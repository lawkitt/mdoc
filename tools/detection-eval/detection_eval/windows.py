"""Model windows over a source, close to the app's (`windows.rs`).

Whole paragraphs packed up to `WINDOW_WORDS` words, each Markdown heading
starting a window with the paragraph after it; a longer paragraph packs its
lines, and only a longer line gets overlapping word windows. A window that
does not fit the model (its `fits` test, e.g. a token limit) is halved with a
quarter overlap until it does. Every model gets the same windows, so the
comparison is between models, not between chunkers.

Offsets are character offsets of the source.
"""

from __future__ import annotations

import re
from collections.abc import Callable

WINDOW_WORDS = 128
LONG_STRIDE = WINDOW_WORDS - 32

_WORD = re.compile(r"\S+")


def _words(source: str) -> list[tuple[int, int]]:
    return [(m.start(), m.end()) for m in _WORD.finditer(source)]


def _paragraphs(source: str) -> list[tuple[int, int, bool]]:
    """(start, end, is_heading) blocks separated by blank lines; a heading
    line is its own block."""
    blocks = []
    pos = 0
    for m in re.finditer(r"(?:\r?\n[ \t]*){2,}|$", source):
        chunk_start, chunk_end = pos, m.start()
        pos = m.end()
        if chunk_start >= chunk_end:
            if m.end() >= len(source):
                break
            continue
        # Split heading lines out of the block.
        line_start = chunk_start
        para_start = None
        for line in source[chunk_start:chunk_end].splitlines(keepends=True):
            line_end = line_start + len(line)
            if line.lstrip().startswith("#"):
                if para_start is not None:
                    blocks.append((para_start, line_start, False))
                    para_start = None
                blocks.append((line_start, line_end, True))
            elif para_start is None:
                para_start = line_start
            line_start = line_end
        if para_start is not None:
            blocks.append((para_start, chunk_end, False))
        if m.end() >= len(source):
            break
    return blocks


def planned_windows(source: str) -> list[tuple[int, int]]:
    words = _words(source)
    if not words:
        return []

    def index(byte: int) -> int:
        lo, hi = 0, len(words)
        while lo < hi:
            mid = (lo + hi) // 2
            if words[mid][0] < byte:
                lo = mid + 1
            else:
                hi = mid
        return lo

    units: list[tuple[int, int, bool]] = []
    heading_start = None
    for start, end, heading in _paragraphs(source):
        begin = heading_start
        heading_start = None
        first = begin if begin is not None else index(start)
        if heading:
            heading_start = first
        else:
            units.append((first, index(end), begin is not None))
    if heading_start is not None:
        units.append((heading_start, len(words), True))

    def lines_of(unit: tuple[int, int]) -> list[tuple[int, int, bool]]:
        at = words[unit[0]][0]
        text = source[at : words[unit[1] - 1][1]]
        out = []
        for line in text.splitlines(keepends=True):
            out.append((index(at), index(at + len(line)), False))
            at += len(line)
        return out

    def pack(items, long):
        windows = []
        open_ = None
        for lo, hi, fresh in items:
            if lo >= hi:
                continue
            if open_ is not None:
                if not fresh and hi - open_[0] <= WINDOW_WORDS:
                    open_ = (open_[0], hi)
                    continue
                windows.append(open_)
                open_ = None
            if hi - lo <= WINDOW_WORDS:
                open_ = (lo, hi)
            else:
                windows.extend(long((lo, hi)))
        if open_ is not None:
            windows.append(open_)
        return windows

    def long_line(line):
        out = []
        start = line[0]
        while True:
            end = min(start + WINDOW_WORDS, line[1])
            out.append((start, end))
            if end == line[1]:
                return out
            start += LONG_STRIDE

    def long_paragraph(paragraph):
        return pack(lines_of(paragraph), long_line)

    windows = pack(units, long_paragraph)
    return [(words[lo][0], words[hi - 1][1]) for lo, hi in windows if lo < hi]


def bounded_windows(
    source: str, fits: Callable[[str], bool] | None = None
) -> list[tuple[int, int]]:
    """Planned windows, each halved (quarter overlap) until `fits` it."""
    pending = list(reversed(planned_windows(source)))
    out = []
    while pending:
        start, end = pending.pop()
        text = source[start:end]
        if fits is None or fits(text):
            out.append((start, end))
            continue
        words = [(start + a, start + b) for a, b in _words(text)]
        if len(words) < 2:
            raise ValueError("a single word does not fit the model")
        size = (len(words) + 1) // 2
        overlap = min(size // 4, size - 1)
        pieces = []
        lo = 0
        while True:
            hi = min(lo + size, len(words))
            pieces.append((words[lo][0], words[hi - 1][1]))
            if hi == len(words):
                break
            lo = hi - overlap
        pending.extend(reversed(pieces))
    out.sort()
    return out
