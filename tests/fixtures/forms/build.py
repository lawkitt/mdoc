#!/usr/bin/env python3
"""Write prompts/<form>.<person>.txt: the note, the blank form and the message.

Usage: python3 tests/fixtures/forms/build.py [--note-dir DIR] [--out DIR]
Every model gets the same text. --note-dir swaps in another note wording
(people/<person>.note.txt under DIR) for a rerun.
"""
import argparse
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
LEAD = "Use no tools: answer from the text below only."


def form_body(path):
    text = path.read_text(encoding="utf-8")
    return text.split("\n---\n", 1)[1].strip()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--note-dir", default=str(HERE))
    ap.add_argument("--out", default=str(HERE / "prompts"))
    args = ap.parse_args()
    cases = json.loads((HERE / "cases.json").read_text(encoding="utf-8"))
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for form_id, people in cases.items():
        if form_id.startswith("_"):
            continue
        body = form_body(HERE / "forms" / f"{form_id}.txt")
        for person, case in people.items():
            note = (Path(args.note_dir) / "people" / f"{person}.note.txt").read_text(encoding="utf-8").strip()
            prompt = f"{LEAD}\n\n{note}\n\n{body}\n\n{case['message']}\n"
            (out / f"{form_id}.{person}.txt").write_text(prompt, encoding="utf-8")


if __name__ == "__main__":
    main()
