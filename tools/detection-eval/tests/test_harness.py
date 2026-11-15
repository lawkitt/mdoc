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

    def test_member_with_itself_is_that_member(self):
        a = [entity(0, 20, "organization"), entity(0, 5, "person"), entity(8, 12, "person", 0.6)]
        self.assertEqual(merge([a, a]), merge([a]))
        self.assertCountEqual(merge([a, a]), a)

    def test_nested_spans_of_the_top_member_survive(self):
        a = [entity(0, 20, "organization"), entity(0, 5, "person")]
        b = [entity(0, 5, "person", 0.5), entity(30, 34, "email")]
        self.assertCountEqual(merge([a, b]), a + [entity(30, 34, "email")])

    def test_third_member_keeps_the_top_rank_category(self):
        a = [entity(0, 4, "person")]
        b = [entity(7, 11, "organization")]
        c = [entity(2, 12, "address")]
        self.assertCountEqual(
            merge([a, b, c]), [entity(2, 12, "person"), entity(7, 11, "organization")]
        )

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


def board(gold, found, proposed, right):
    """One option's holdout and real set, English only, split in halves."""
    half = [gold // 2, found // 2, proposed // 2, right // 2]
    return {s: {("en", "PERSON"): list(half)} for s in ("holdout", "real")}


class Verdict(unittest.TestCase):
    def setUp(self):
        from detection_eval import report

        self.report = report
        self.boards = {
            "default": board(100, 86, 100, 56),
            "a": board(100, 92, 100, 50),
            "b": board(100, 70, 100, 90),
        }

    def test_pass_and_fail(self):
        self.assertEqual(self.report.verdict("a", self.boards), "pass")
        self.assertIn("recall < 90", self.report.verdict("b", self.boards))
        boards = dict(self.boards, c=board(100, 96, 100, 40))
        self.assertEqual(self.report.verdict("c", boards), "precision < default − 10")

    def test_no_default_is_incomplete(self):
        boards = {k: v for k, v in self.boards.items() if k != "default"}
        self.assertTrue(self.report.verdict("a", boards).startswith("incomplete"))

    def test_missing_set_is_incomplete(self):
        boards = dict(self.boards, a={"holdout": self.boards["a"]["holdout"]})
        self.assertTrue(self.report.verdict("a", boards).startswith("incomplete"))

    def test_combo_needs_every_member_and_a_gain(self):
        self.assertTrue(self.report.verdict("a+b", dict(self.boards, **{"a+b": board(100, 93, 100, 50)}))
                        .startswith("gain"))
        boards = {k: v for k, v in self.boards.items() if k != "b"}
        boards["a+b"] = board(100, 98, 100, 50)
        self.assertTrue(self.report.verdict("a+b", boards).startswith("incomplete"))


class Decode(unittest.TestCase):
    def test_cut_drops_low_spans_before_joining(self):
        import numpy as np

        from detection_eval.adapters import decode_tags

        text = "Ann Lee"
        offsets = [(0, 0), (0, 3), (3, 7), (0, 0)]
        id2label = {0: "O", 1: "B-NAME", 2: "I-NAME"}
        probs = np.array([[1, 0, 0], [0.1, 0.9, 0], [0.6, 0, 0.4], [1, 0, 0]], dtype=float)
        labels = {"NAME": "person"}
        # Argmax: "Ann" (B), then O for " Lee".
        self.assertEqual([(e.start, e.end) for e in decode_tags(text, offsets, probs, id2label, labels)], [(0, 3)])
        self.assertEqual(decode_tags(text, offsets, probs, id2label, labels, cut=0.95), [])


class Staged(unittest.TestCase):
    def test_failed_run_keeps_previous_and_success_drops_stale(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "set"
            target.mkdir()
            (target / "old.md").write_text("old", encoding="utf-8")
            with self.assertRaises(RuntimeError):
                with fixtures.staged(target) as partial:
                    (partial / "new.md").write_text("new", encoding="utf-8")
                    raise RuntimeError
            self.assertEqual(sorted(p.name for p in target.iterdir()), ["old.md"])
            with fixtures.staged(target) as partial:
                (partial / "new.md").write_text("new", encoding="utf-8")
            self.assertEqual(sorted(p.name for p in target.iterdir()), ["new.md"])
            self.assertFalse(fixtures.complete(target))


class OutRoot(unittest.TestCase):
    def test_relative_out_is_absolute(self):
        from detection_eval.run import out_root

        self.assertTrue(out_root("runs").is_absolute())


class Join(unittest.TestCase):
    def test_separately_tagged_people_stay_apart(self):
        import numpy as np

        from detection_eval.adapters import decode_tags

        text = "Ann Bo"
        offsets = [(0, 3), (3, 6)]
        id2label = {0: "O", 1: "B-NAME", 2: "I-NAME", 3: "B-SURNAME"}
        apart = np.array([[0, 0.9, 0.1, 0], [0, 0.9, 0.1, 0]], dtype=float)
        parts = np.array([[0, 0.9, 0.1, 0], [0, 0.1, 0, 0.9]], dtype=float)
        labels = {"NAME": "person", "SURNAME": "person"}
        spans = lambda probs: [(e.start, e.end) for e in decode_tags(text, offsets, probs, id2label, labels)]  # noqa: E731
        self.assertEqual(spans(apart), [(0, 3), (4, 6)])
        self.assertEqual(spans(parts), [(0, 6)])


class Viterbi(unittest.TestCase):
    def test_path_differs_from_argmax_when_argmax_breaks_bioes(self):
        import numpy as np

        from detection_eval.adapters import decode_tags, viterbi_tags

        id2label = {0: "O", 1: "B-P", 2: "I-P", 3: "E-P", 4: "S-P"}
        # Argmax: O, I-P, O, an I with no B. The best path that opens with B
        # and closes with E: B-P, E-P, O.
        probs = np.array(
            [[0.6, 0.3, 0.05, 0.0, 0.05], [0.1, 0.0, 0.5, 0.4, 0.0], [0.9, 0.0, 0.0, 0.0, 0.1]]
        )
        self.assertEqual(probs.argmax(axis=-1).tolist(), [0, 2, 0])
        self.assertEqual(viterbi_tags(probs, id2label), [1, 3, 0])
        text = "Ann Lee ok"
        offsets = [(0, 3), (3, 7), (7, 10)]
        found = decode_tags(text, offsets, probs, id2label, {"P": "person"}, decoder="viterbi")
        self.assertEqual([(e.start, e.end) for e in found], [(0, 7)])


class Corpus(unittest.TestCase):
    def test_changed_document_changes_the_id(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "a.md").write_text("one", encoding="utf-8")
            before = fixtures.corpus_id(root)
            (root / "a.md").write_text("two", encoding="utf-8")
            self.assertNotEqual(fixtures.corpus_id(root), before)
