"""Harness self-tests that need no model: `python -m unittest discover tests`
from tools/detection-eval/."""

import re
import tempfile
import unittest
from pathlib import Path

from detection_eval import fixtures
from detection_eval.combo import merge
from detection_eval.windows import bounded_windows


def entity(start, end, label, score=0.9):
    return {"start": start, "end": end, "label": label, "score": score}


class Merge(unittest.TestCase):
    def test_one_member_groups_pass_unchanged(self):
        a = [entity(0, 4, "person"), entity(0, 4, "organization", 0.6)]
        b = [entity(10, 14, "email")]
        self.assertCountEqual(merge([a, b]), a + b)

    def test_overlap_takes_longest_span_and_top_members_labels(self):
        a = [entity(5, 9, "person", 0.8), entity(5, 9, "organization", 0.7)]
        b = [entity(0, 9, "address", 0.95)]
        self.assertCountEqual(
            merge([a, b]),
            [entity(0, 9, "person", 0.8), entity(0, 9, "organization", 0.7)],
        )

    def test_chain_through_another_member_is_one_group(self):
        a = [entity(0, 5, "person"), entity(8, 12, "person", 0.6)]
        b = [entity(3, 10, "person", 0.5)]
        self.assertEqual(merge([a, b]), [entity(3, 10, "person", 0.9)])

    def test_tie_keeps_higher_ranked_span(self):
        a = [entity(0, 5, "person")]
        b = [entity(2, 7, "organization")]
        self.assertEqual(merge([a, b]), [entity(0, 5, "person")])


class Sources(unittest.TestCase):
    def test_front_matter_and_gold_removed(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "x.md"
            path.write_text(
                "---\nid: x\nlanguage: en\n---\nTo ⟦PERSON|Анна Ли⟧ at ⟦EMAIL|a@b.invalid⟧.\n",
                encoding="utf-8",
                newline="\n",
            )
            self.assertEqual(fixtures.source_of(path), "To Анна Ли at a@b.invalid.\n")

    def test_snapshot_offsets_are_utf8_bytes(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "x.md"
            source = "Ён Ли: a@b.invalid"
            fixtures.write_snapshot(path, source, "m", 0.5, [fixtures.Entity(0, 5, "person", 0.9)])
            row = fixtures.read_snapshot(path)["entities"][0]
            self.assertEqual((row["start"], row["end"], row["text"]), (0, 9, "Ён Ли"))
            self.assertEqual(source.encode()[row["start"] : row["end"]].decode(), "Ён Ли")


class Windows(unittest.TestCase):
    def test_every_word_of_every_fixture_is_in_a_window(self):
        for path in fixtures.fixture_paths(fixtures.sets()["fixtures"]):
            source = fixtures.source_of(path)
            covered = set()
            for start, end in bounded_windows(source, lambda t: len(t.split()) <= 64):
                covered.update(start + m.start() for m in re.finditer(r"\S+", source[start:end]))
            missing = [m.group() for m in re.finditer(r"\S+", source) if m.start() not in covered]
            self.assertEqual(missing, [], path.name)


if __name__ == "__main__":
    unittest.main()
