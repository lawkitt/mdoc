"""dpi.py's line and number recall: `python3 -m unittest discover tools/ocr-upstream/tests`."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from dpi import recall  # noqa: E402


class Recall(unittest.TestCase):
    def test_a_line_needs_whole_words(self):
        self.assertEqual(recall("25.", "125.")["lines"], 0)
        self.assertEqual(recall("25.", "see 25. here")["lines"], 1)

    def test_each_output_occurrence_counts_once(self):
        found = recall("Payment 100\nPayment 100", "Payment 1000")
        self.assertEqual((found["lines"], found["numbers"]), (0, 0))
        found = recall("Payment 100\nPayment 100", "Payment 100")
        self.assertEqual((found["lines"], found["numbers"]), (1, 1))
        found = recall("Payment 100\nPayment 100", "Payment 100 Payment 100")
        self.assertEqual((found["lines"], found["numbers"]), (2, 2))

    def test_a_short_line_cannot_take_a_longer_lines_words(self):
        found = recall("rent 22.34\n01.01.2026 Invoice rent 22.34", "01.01.2026 Invoice rent 22.34")
        self.assertEqual(found["lines"], 1)


if __name__ == "__main__":
    unittest.main()
