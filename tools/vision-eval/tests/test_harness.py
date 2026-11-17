"""Self-tests for the vision evaluation harness; no model or native host."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from vision_eval import classes, compare, geometry, score  # noqa: E402


class Geometry(unittest.TestCase):
    def test_mirage_boxes_scale_each_axis_from_1000(self):
        self.assertEqual(geometry.mirage_box([100, 500, 1000, 1000], 1241, 1755),
                         [124.1, 877.5, 1241.0, 1755.0])

    def test_iou_and_greedy_match(self):
        a, b = [0, 0, 10, 10], [5, 0, 15, 10]
        self.assertAlmostEqual(geometry.iou(a, b), 50 / 150)
        gts = [[0, 0, 10, 10], [100, 100, 110, 110]]
        preds = [[101, 100, 111, 110], [0, 0, 10, 9], [0, 0, 10, 10]]
        self.assertEqual(sorted(geometry.match(gts, preds, 0.5)), [(0, 2), (1, 0)])

    def test_seal_cer_ignores_spacing_bullets_quotes_and_case(self):
        e, n = geometry.seal_cer(["ООО «Ромашка» • ПЕЧАТЬ", "ИНН 7701234567"], 'ооо"ромашка" печать инн 7701234567')
        self.assertEqual((e, n), (0, len("ооо\"ромашка\"печатьинн7701234567")))
        e, _ = geometry.seal_cer(["ИНН 7701234567"], "ИНН 77012З4567")  # a Cyrillic З for 3
        self.assertEqual(e, 1)

    def test_seal_cer_counts_missing_and_extra_text(self):
        self.assertEqual(geometry.seal_cer(["ABC", "DEF"], ""), (6, 6))
        self.assertEqual(geometry.seal_cer(["ABC"], "ABC XY"), (2, 3))


class Classes(unittest.TestCase):
    def test_mirage_names_map_on_whole_words(self):
        self.assertEqual(classes.mirage_class("stamp"), "seal")
        self.assertEqual(classes.mirage_class("handwritten signature"), "signature")
        self.assertEqual(classes.mirage_class("QR code"), "qr_code")
        self.assertEqual(classes.mirage_class("text block"), None)
        self.assertEqual(classes.mirage_class("headline"), None)  # not "head"

    def test_rxing_formats(self):
        self.assertEqual(classes.rxing_class("QR_CODE"), "qr_code")
        self.assertEqual(classes.rxing_class("DATA_MATRIX"), "qr_code")
        self.assertEqual(classes.rxing_class("CODE_128"), "barcode")


def page(artifacts, words):
    return ({"id": "p", "lang": "en", "truth": {"artifacts": artifacts}},
            {"outputs": {"p": {"words": [{"text": "w", "box": b} for b in words]}}, "page_ms": {"p": 100}})


class TextLoss(unittest.TestCase):
    def test_words_under_a_wrong_box_are_lost_and_under_a_true_artifact_are_not(self):
        p, ocr_run = page([{"class": "seal", "box": [0, 0, 100, 100]}],
                          [[10, 10, 30, 20], [200, 10, 240, 20], [400, 10, 440, 20]])
        run = {"outputs": {"p": {"items": [
            {"label": "seal", "class": "seal", "score": 0.9, "box": [0, 0, 250, 100]},   # oversized seal
            {"label": "header", "class": "header", "score": 0.9, "box": [390, 0, 450, 30]},  # headers replace nothing
        ]}}}
        loss = score.text_loss([p], ocr_run, run, "layout")
        self.assertEqual(loss, {"reference_words": 2, "checked_pages": 0, "lost": {"seal": 1}, "lost_total": 1})

    def test_ground_truth_text_keeps_only_its_words_as_reference(self):
        p, ocr_run = page([], [[10, 10, 30, 20], [200, 10, 240, 20]])
        p["truth"]["text"] = "W"  # one true word; the second OCR "w" is junk
        run = {"outputs": {"p": {"objects": [
            {"name": "signature", "class": "signature", "score": 0.5, "box": [0, 0, 300, 30]}]}}}
        loss = score.text_loss([p], ocr_run, run, "mirage")
        self.assertEqual((loss["reference_words"], loss["checked_pages"], loss["lost_total"]), (1, 1, 1))

    def test_layout_scores_below_the_cut_are_ignored(self):
        p, ocr_run = page([], [[10, 10, 30, 20]])
        run = {"outputs": {"p": {"items": [{"label": "seal", "class": "seal", "score": 0.49, "box": [0, 0, 50, 50]}]}}}
        self.assertEqual(score.text_loss([p], ocr_run, run, "layout")["lost_total"], 0)


class Detection(unittest.TestCase):
    def test_counts_per_class_and_language(self):
        p = {"id": "p", "lang": "ru", "truth": {"artifacts": [
            {"class": "seal", "box": [0, 0, 100, 100]}, {"class": "signature", "box": [200, 0, 300, 50]}]}}
        run = {"outputs": {"p": {"objects": [
            {"name": "stamp", "class": "seal", "score": 0.7, "box": [5, 5, 100, 100]},
            {"name": "signature", "class": "signature", "score": 0.6, "box": [600, 0, 700, 50]}]}}}
        c = score.detection_scores([p], run, "mirage")
        self.assertEqual((c[("seal", "all")]["tp"], c[("seal", "ru")]["fp"]), (1, 0))
        self.assertEqual((c[("signature", "all")]["fp"], c[("signature", "all")]["fn"]), (1, 1))


class SealText(unittest.TestCase):
    def test_an_inlined_reading_without_a_seal_fails(self):
        p = {"id": "p", "lang": "ru", "truth": {"artifacts": [
            {"class": "seal", "box": [0, 0, 100, 100], "segments": ["ИНН 7701234567"]}]}}
        run = {"outputs": {"p": {"seals": [
            {"box": [0, 0, 100, 100], "text": "ИНН 7701234567", "conf": 0.95, "inline": True},
            {"box": [500, 500, 600, 600], "text": "BOGUS", "conf": 0.95, "inline": True}]}}}
        st = score.seal_text([p], run)
        self.assertEqual((st["inlined"], st["inlined_cer"], st["inlined_without_seal"], st["inlined_over_bar"]),
                         (1, 0.0, 1, 1))


class Coverage(unittest.TestCase):
    def test_a_page_without_output_makes_the_run_incomplete(self):
        pages_ = [{"id": "a"}, {"id": "b"}, {"id": "c"}]
        run = {"outputs": {"a": {}, "c": {"error": "not a PPM"}}, "page_ms": {"a": 1.0}}
        self.assertEqual(score.missing(pages_, run), 1)


class Compare(unittest.TestCase):
    def test_canonical_rounds_boxes_and_scores(self):
        raw = {"codes": [{"format": "QR_CODE", "text": "x", "box": [10.4, 10.6, 20.5, 30.49]}]}
        self.assertEqual(compare.canonical("rxing", raw), [["QR_CODE", "x", [10, 11, 20, 30], 1.0]])


if __name__ == "__main__":
    unittest.main()
