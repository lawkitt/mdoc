"""Summarize an Animation Hitches trace of mdoc (standard library + xcrun only).

Usage: python3 -I analyze.py <trace> <pid> <name:start:end>...

Phases are seconds from the start of the recording (see the per-second frame
timeline this prints). For each phase: presented frames and the gaps between
them, and the main thread's CPU per run-loop pass (Running time between two
waits for events), which is the native analogue of a headless frame time.
"""
import bisect
import subprocess
import sys
import xml.etree.ElementTree as ET
from collections import Counter


def table(trace, schema):
    xpath = f'/trace-toc/run[@number="1"]/data/table[@schema="{schema}"]'
    out = subprocess.run(['xcrun', 'xctrace', 'export', '--input', trace, '--xpath', xpath],
                         check=True, capture_output=True).stdout
    ids = {}

    def resolve(e):
        if 'ref' in e.attrib:
            return ids[e.attrib['ref']]
        if 'id' in e.attrib:
            ids[e.attrib['id']] = e
        for child in e:
            resolve(child)
        return e
    for row in ET.fromstring(out).iter('row'):
        yield [resolve(cell) for cell in row]


def quantile(values, q):
    return values[min(len(values) - 1, int(len(values) * q))]


def main(trace, pid, phases):
    phases = [(n, float(a) * 1e9, float(b) * 1e9) for n, a, b in (p.split(':') for p in phases)]
    hitches = sum(1 for _ in table(trace, 'hitches-summary'))
    frames = sorted(int(c[0].text) for c in table(trace, 'displayed-surfaces-interval')
                    if f'({pid})' in c[9].attrib.get('fmt', ''))
    per_second = Counter(t // 1_000_000_000 for t in frames)
    print(f'hitches={hitches} mdoc_frames={len(frames)}')
    print('frames/s: ' + ' '.join(f'{s}:{per_second[s]}' for s in range(max(per_second) + 1)))

    waits, main_tid = [], None
    for c in table(trace, 'runloop-events'):
        if c[7].text != '1' or 'mdoc' not in c[8].attrib.get('fmt', ''):
            continue
        main_tid = c[8].find('tid').text
        if c[2].text == 'waiting_for_events':
            waits.append((int(c[0].text), c[3].attrib.get('fmt')))
    passes, last_end = [], None
    for t, edge in sorted(waits):
        if edge == 'START' and last_end is not None:
            passes.append((last_end, t))
        elif edge == 'END':
            last_end = t
    running = sorted((int(c[0].text), int(c[0].text) + int(c[3].text))
                     for c in table(trace, 'thread-state')
                     if c[1].tag == 'thread' and c[1].find('tid') is not None
                     and c[1].find('tid').text == main_tid and c[2].text == 'Running')
    starts = [r[0] for r in running]

    def cpu(a, b):
        i, total = max(0, bisect.bisect_left(starts, a) - 1), 0
        while i < len(running) and running[i][0] < b:
            total += max(0, min(b, running[i][1]) - max(a, running[i][0]))
            i += 1
        return total

    for name, lo, hi in phases:
        seg = [t for t in frames if lo <= t < hi]
        gaps = sorted((b - a) / 1e6 for a, b in zip(seg, seg[1:]))
        work = sorted(x for x in (cpu(a, b) / 1e6 for a, b in passes if lo <= a < hi) if x > 0.05)
        print(f'{name}: frames={len(seg)} fps={len(seg) / ((hi - lo) / 1e9):.1f} '
              f'gaps_over_one_refresh={sum(g > 17.5 for g in gaps)} max_gap={gaps[-1]:.1f}ms | '
              f'main-thread CPU per pass: median={quantile(work, .5):.2f} p95={quantile(work, .95):.2f} '
              f'p99={quantile(work, .99):.2f} max={work[-1]:.2f} ms, over 16.7 ms={sum(x > 16.7 for x in work)}')


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2], sys.argv[3:])
