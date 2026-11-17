"""Self-tests for the vision evaluation harness; no model or native host."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from vision_eval import classes, compare, geometry, mirage, score, tune  # noqa: E402


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


class GroundTruthAlignment(unittest.TestCase):
    def test_a_hallucinated_duplicate_does_not_stand_in_for_the_real_word(self):
        # OCR reads a junk "PAYMENT" in a margin above the real one; ground truth has one.
        p = {"id": "p", "lang": "en", "truth": {"artifacts": [], "text": "Payment due today"}}
        words = [{"text": "PAYMENT", "box": [900, 10, 980, 20]},
                 {"text": "Payment", "box": [10, 100, 80, 110]}, {"text": "due", "box": [90, 100, 120, 110]},
                 {"text": "today", "box": [130, 100, 180, 110]}]
        ocr_run = {"outputs": {"p": {"words": words}}, "page_ms": {"p": 1}}
        run = {"outputs": {"p": {"items": [
            {"label": "seal", "class": "seal", "score": 0.9, "box": [0, 90, 85, 120]}]}}}
        loss = score.text_loss([p], ocr_run, run, "layout")
        self.assertEqual(loss["lost_total"], 1)  # the real "Payment" under the seal box


class Timing(unittest.TestCase):
    def test_only_the_sets_pages_count(self):
        sp = [{"id": "a"}]
        run = {"page_ms": {"a": 100.0, "stale": 0.0}, "peak_bytes": 0, "idle_peak_bytes": 0}
        ocr_run = {"page_ms": {"a": 100.0, "stale": 10000.0}}
        self.assertAlmostEqual(score.timing(sp, run, ocr_run)["overhead"], 1.0)


class Names(unittest.TestCase):
    def test_unlisted_names_are_counted_as_other(self):
        run = {"outputs": {"p": {"objects": [{"name": "text block"}, {"name": "Romashka LLC sign"}]}}}
        self.assertEqual(dict(score.mirage_names([{"id": "p"}], run)), {"text block": 1, "other": 1})


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


class MirageHost(unittest.TestCase):
    def test_bare_nan_scores_become_null(self):
        from vision_eval.worker import parse_host_line
        line = '{"result":{"objects":[{"id":1,"score":-nan(ind),"bbox":[1,2,3,4]},{"score":nan}],"x":[nan, 1]}}'
        data, count = parse_host_line(line)
        self.assertEqual(count, 3)
        self.assertIsNone(data["result"]["objects"][0]["score"])
        self.assertEqual(data["result"]["x"], [None, 1])

    def test_nan_inside_a_string_is_untouched(self):
        from vision_eval.worker import parse_host_line
        data, count = parse_host_line('{"text":[{"string":"value: nan, tail \\" nan","score":nan}]}')
        self.assertEqual(count, 1)
        self.assertEqual(data["text"][0]["string"], 'value: nan, tail " nan')

    def test_words_containing_nan_are_untouched(self):
        from vision_eval.worker import parse_host_line
        data, count = parse_host_line('{"text":[{"string":"financial nan","score":0.5}]}')
        self.assertEqual((count, data["text"][0]["string"]), (0, "financial nan"))


class Compare(unittest.TestCase):
    def test_canonical_rounds_boxes_and_scores(self):
        raw = {"codes": [{"format": "QR_CODE", "text": "x", "box": [10.4, 10.6, 20.5, 30.49]}]}
        self.assertEqual(compare.canonical("rxing", raw), [["QR_CODE", "x", [10, 11, 20, 30], 1.0]])

    def test_seal_inline_decision_is_part_of_the_digest(self):
        a = {"seals": [{"text": "X", "box": [0, 0, 1, 1], "conf": 0.8999, "inline": False}]}
        b = {"seals": [{"text": "X", "box": [0, 0, 1, 1], "conf": 0.9001, "inline": True}]}
        self.assertNotEqual(compare.canonical("seal", a), compare.canonical("seal", b))


class MirageOptions(unittest.TestCase):
    def test_known_options_pass_and_unknown_keys_are_rejected(self):
        ok = {"sections": ["objects"], "conf": 0.05, "agnostic_nms": True, "max_objects": 64}
        self.assertEqual(mirage.check_options(ok), ok)
        for bad in ({"nms": True}, {"sections": ["objects"], "confidence": 0.1}, {"form": {}},
                    {"sections": ["bogus"]}, {"sections": []}, {"conf": 5}, {"conf": True},
                    {"agnostic_nms": 1}, {"max_tokens": 0}, {"mask_format": "polygon"}):
            with self.assertRaises(ValueError, msg=bad):
                mirage.check_options(bad)

    def test_output_must_show_the_options_took_effect(self):
        opts = {"sections": ["objects"], "conf": 0.2}
        mirage.check_applied(opts, {"objects": [{"score": 0.2}, {"score": 0.9}]})
        with self.assertRaises(AssertionError):
            mirage.check_applied(opts, {"objects": [], "text": [{"string": "x"}]})
        with self.assertRaises(AssertionError):
            mirage.check_applied(opts, {"objects": [{"score": 0.1}]})

    def test_every_tune_variant_runs_objects_only_with_valid_options(self):
        ids = set()
        for v in tune.variants():
            self.assertEqual(tune.options(v)["sections"], ["objects"])
            ids.add(tune.variant_id(v))
        self.assertEqual(len(ids), len(tune.variants()))


class Tuning(unittest.TestCase):
    def test_bottom_tiles_cover_the_bottom_half_at_about_twice_the_scale(self):
        t = tune.tiles(1275, 1650, "bottom2x")
        self.assertEqual(t, [(0, 825, 765, 1650), (510, 825, 1275, 1650)])
        for x0, y0, x1, y1 in t:  # Mirage's canvas: long side 512
            self.assertGreater((512 / max(x1 - x0, y1 - y0)) / (512 / 1650), 1.9)
        self.assertEqual(tune.tiles(1275, 1650, None), [])

    def test_crop_candidates_prefer_unnamed_seal_sized_boxes(self):
        objs = [{"name": "stamp", "score": 0.9, "box": [100, 100, 200, 200]},
                {"name": "sign", "score": 0.5, "box": [400, 100, 500, 190]},
                {"name": "text block", "score": 0.8, "box": [0, 300, 1200, 400]},  # too wide
                {"name": "number", "score": 0.4, "box": [402, 101, 501, 190]},     # duplicate of the sign
                {"name": "sticker", "score": 0.3, "box": [600, 600, 680, 680]}]
        self.assertEqual(tune.crop_candidates(objs, 1275, 1650), [1, 4])
        self.assertEqual(tune.crop_candidates(objs, 1275, 1650, 1), [1])
        words = [{"box": [405, 105, 495, 185]}]  # the sign is covered by OCR words
        self.assertEqual(tune.crop_candidates(objs, 1275, 1650, 2, words), [4, 1])
        self.assertEqual(tune.crop_box([100, 100, 200, 140], 0.5, 1275, 1650), (50, 50, 251, 191))

    def test_roundness(self):
        circle = [v for a in range(64) for v in (100 + 50 * __import__("math").cos(a / 64 * 6.2832),
                                                  100 + 50 * __import__("math").sin(a / 64 * 6.2832))]
        self.assertGreater(tune.roundness(circle), 0.99)
        self.assertAlmostEqual(tune.roundness([0, 0, 10, 0, 10, 10, 0, 10]), 0.785, places=3)
        self.assertIsNone(tune.roundness(None))

    def test_rules_filter_then_suppress_and_score(self):
        f = lambda box, named, ocr=0.0: {"box": box, "score": 0.5, "pools": {"d33": named, "d33+crop": named, "any": True},
                                         "aspect": 1.0, "side": 0.05, "round": None, "ocr": ocr, "colour": 0.0}
        prep = [{"tier": "real", "seals": [[0, 0, 100, 100]], "refs": [[300, 300, 340, 320]],
                 "feats": [f([0, 0, 100, 100], True), f([2, 0, 100, 100], True), f([290, 290, 400, 400], False, 0.6)]}]
        e = tune.evaluate(prep, "d33", {})
        self.assertEqual((e["tp"], e["fp"], e["lost"]), (1, 0, 0))  # the duplicate is suppressed
        e = tune.evaluate(prep, "any", {})
        self.assertEqual((e["tp"], e["fp"], e["lost"]), (1, 1, 1))
        e = tune.evaluate(prep, "any", {"max_ocr": 0.4})
        self.assertEqual((e["tp"], e["fp"], e["lost"]), (1, 0, 0))

    def test_roles_keep_tuning_and_final_sets_apart(self):
        tune.require_role({"name": "tune-seal", "role": "tune"}, True)
        tune.require_role({"name": "en-recap"}, False)
        with self.assertRaises(SystemExit):
            tune.require_role({"name": "en-recap"}, True)
        with self.assertRaises(SystemExit):
            tune.require_role({"name": "tune-seal", "role": "tune"}, False)

    def test_a_saved_setting_must_match_what_final_runs(self):
        from vision_eval import d33
        from vision_eval.worker import MIRAGE_KEYWORDS
        v = [x for x in tune.variants() if tune.variant_id(x) == "c10-kw-crop1p25"][0]
        good = {"variant": v, "variant_id": "c10-kw-crop1p25", "options": tune.options(v),
                "keywords": MIRAGE_KEYWORDS, "name_map": {"seal": dict(d33.D33_MAP)["seal"]}, "nms_iou": 0.5,
                "crop_rule": {"per_page": 1, "side": [0.02, 0.30], "max_aspect": 3.0}, "pool": "d33",
                "rules": {k: None for k in tune.RULE_GRID}}
        self.assertEqual(tune.setting_problems(good), [])
        for key, bad in (("keywords", ["stamp"]), ("nms_iou", 0.3), ("crop_rule", {"per_page": 2}),
                         ("pool", "all"), ("rules", {"max_aspect": 9.0})):
            self.assertTrue(tune.setting_problems(dict(good, **{key: bad})), key)

    def test_pages_without_provenance_are_found(self):
        import json, tempfile
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "sources.json").write_text(json.dumps({"a": {"item": "x"}, "b": {"note": "?"}}))
            self.assertEqual(tune.page_source_ids(Path(d), ["a", "b", "c"]), {"a": {"item:x"}})

    def test_source_ids_name_the_docket_behind_a_page(self):
        import json, tempfile
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "sources.json").write_text(json.dumps({
                "a": {"file": "gov.uscourts.x.1__gov.uscourts.x.1.5.0__p2.png"},
                "b": {"item": "gov.uscourts.x.2", "url": "https://example.org/b"}}))
            self.assertEqual(tune.source_ids(Path(d)),
                             {"item:gov.uscourts.x.1", "item:gov.uscourts.x.2", "url:https://example.org/b"})


if __name__ == "__main__":
    unittest.main()
