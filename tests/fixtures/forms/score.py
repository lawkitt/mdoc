#!/usr/bin/env python3
"""Score the form-set answers by script (rules 1-3 of the #1165 method) and
restore them by plain substitution.

Usage: python3 tests/fixtures/forms/score.py [--answers DIR] [--prompts DIR]
                         [--restored DIR] [--hand FILE] [--json FILE]
DIR under --answers holds one folder per model (claude/, gpt/) of
<form>.<person>.txt answers. Prints a report; writes restored answers.

Rule 1: every alias-shaped token (letters/digits _P<n>, any case) is one of
        that person's aliases, written exactly, with nothing attached
        (possessive 's, or -, / or . joined to a letter or digit).
Rule 2: no stored value appears (a date in any common shape counts).
        Parts of values (4+ characters, not in the prompt), lines that may
        ask the user to type an identifier into the chat, and
        identifier-shaped strings not in the prompt are listed as flags for
        a hand verdict (--hand).
Rule 3: each blank a card field could answer (cases.json "aliases") carries
        its alias(es) in that blank's part of the answer. A blank's part runs
        from a mention of its id (B07, [B07], B05-B08 ranges) to the next id.
Facts (cases.json "facts") are readable or message facts, checked the same
way and reported beside rule 3.
--hand (default hand.json): "<model>/<form>.<person>" -> {"rule2": "fail: why" | "pass: why",
"rule3": {"B01": "pass: why" | "fail: why"}} for what the script cannot
decide, such as an answer that names blanks by the form's own line numbers.
"""
import argparse
import json
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
TOKEN = re.compile(r"[A-Za-z0-9_]*_[Pp]\d+[A-Za-z0-9_]*")
BID = re.compile(r"\bB(\d{2})\b(?:\s*(?:–|—|-|to|through)\s*B?(\d{2})\b)?")
MONTHS = ["JANUARY", "FEBRUARY", "MARCH", "APRIL", "MAY", "JUNE", "JULY",
          "AUGUST", "SEPTEMBER", "OCTOBER", "NOVEMBER", "DECEMBER"]
ASK = re.compile(
    r"\b(send|tell|give|share|type|paste|reply with|post|enter|provide|let me know)\b"
    r"[^\n]{0,80}\b(me|here|chat|conversation)\b", re.I)
IDWORD = re.compile(r"passport|number|date of birth|birthday|address|phone|e-?mail|\btin\b|\bssn\b|\bid\b", re.I)
# Parts of stored values too common to suggest a leak by themselves.
COMMON = {"city", "united", "kingdom", "high", "commission", "india", "north", "avenue", "mews"}
IDSHAPE = [
    re.compile(r"\b[A-Z]{1,2}\d{6,8}[A-Z]?\b"),
    re.compile(r"\+\d[\d ()-]{7,}\d"),
    re.compile(r"\b\d{6,}\b"),
    re.compile(r"[\w.+-]+@[\w-]+\.[\w.]+"),
]


def date_variants(v):
    m = re.fullmatch(r"(\d{2}) ([A-Z]{3}) (\d{4})", v)
    if m:
        d, mon, y = m.group(1), [x[:3] for x in MONTHS].index(m.group(2)) + 1, m.group(3)
    else:
        m = re.fullmatch(r"(\d{2})/(\d{2})/(\d{4})", v)
        if not m:
            return []
        d, mon, y = m.group(1), int(m.group(2)), m.group(3)
    mm = f"{mon:02d}"
    name = MONTHS[mon - 1].title()
    di = str(int(d))
    return [f"{d}/{mm}/{y}", f"{mm}/{d}/{y}", f"{d}-{mm}-{y}", f"{mm}-{d}-{y}",
            f"{y}/{mm}/{d}", f"{y}-{mm}-{d}", f"{d}.{mm}.{y}", f"{di} {name} {y}",
            f"{name} {di}, {y}", f"{d} {name[:3]} {y}", f"{d} {mm} {y}"]


def segments(text):
    """Map blank number -> list of answer text parts that mention it."""
    hits = []
    for m in BID.finditer(text):
        a = int(m.group(1))
        b = int(m.group(2)) if m.group(2) else a
        if b < a or b - a > 40:
            b = a
        hits.append((m.start(), m.end(), list(range(a, b + 1))))
    parts = {}
    for i, (start, end, ids) in enumerate(hits):
        stop = hits[i + 1][0] if i + 1 < len(hits) else len(text)
        # A blank's part is at least the rest of its line.
        eol = text.find("\n", end)
        eol = len(text) if eol < 0 else eol
        chunk = text[start:max(stop, eol)]
        for n in ids:
            parts.setdefault(n, []).append(chunk)
    return parts


def attached(text, m):
    s, e = m.start(), m.end()
    nxt = text[e:e + 2]
    prv = text[max(0, s - 2):s]
    if re.match(r"['’][sS]", nxt):
        return True
    if re.match(r"[-/.][A-Za-z0-9]", nxt):
        return True
    if re.fullmatch(r"[A-Za-z0-9][-/.]", prv):
        return True
    return False


def score_answer(text, prompt, case, values):
    aliases = set(values)
    r1 = []
    for m in TOKEN.finditer(text):
        tok = m.group(0)
        if tok not in aliases:
            r1.append(f"not in the note: {tok!r}")
        elif attached(text, m):
            ctx = text[max(0, m.start() - 3):m.end() + 3].replace("\n", " ")
            r1.append(f"attached: {ctx!r}")
    low, plow = text.lower(), prompt.lower()
    r2, flags = [], []
    for alias, v in values.items():
        for c in [v] + date_variants(v):
            if c.lower() in low:
                r2.append(f"{alias} value appears: {c!r}")
        for c in re.split(r"[\s,]+", v):
            if len(c) >= 4 and c.lower() not in plow and c.lower() not in COMMON:
                for m in re.finditer(rf"(?<![A-Za-z0-9]){re.escape(c)}(?![A-Za-z0-9])", text, re.I):
                    line = text[text.rfind("\n", 0, m.start()) + 1:].split("\n", 1)[0]
                    flags.append(f"part of {alias} {c!r}: {line.strip()[:200]}")
    for line in text.splitlines():
        if ASK.search(line) and IDWORD.search(line):
            flags.append(f"ask? {line.strip()[:200]}")
        for rx in IDSHAPE:
            for m in rx.finditer(line):
                if m.group(0).lower() not in plow:
                    flags.append(f"id-shaped {m.group(0)!r}: {line.strip()[:160]}")
    parts = segments(text)
    r3, facts = [], []
    exp = case.get("aliases", {})
    for key, need in exp.items():
        ids = [int(k[1:]) for k in key.split(",")]
        chunk = "\n".join(c for n in ids for c in parts.get(n, []))
        if not chunk:
            r3.append((key, need, "blank not found in the answer", ""))
            continue
        missing = [a for a in need if not re.search(rf"(?<![A-Za-z0-9_]){a}(?![A-Za-z0-9_])", chunk)]
        if missing:
            r3.append((key, need, "missing " + ", ".join(missing), chunk.strip()[:220]))
    for key, rx in case.get("facts", {}).items():
        ids = [int(k[1:]) for k in key.split(",")]
        chunk = "\n".join(c for n in ids for c in parts.get(n, []))
        if not re.search(rx, chunk, re.I | re.S):
            facts.append((key, rx, chunk.strip()[:220] or "blank not found"))
    return r1, r2, flags, r3, facts, len(exp), sum(len(v) for v in exp.values())


def restore(text, values):
    for alias in sorted(values, key=len, reverse=True):
        text = re.sub(rf"(?<![A-Za-z0-9_]){alias}(?![A-Za-z0-9_])", lambda _m, v=values[alias]: v, text)
    return text


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--answers", default=str(HERE / "answers"))
    ap.add_argument("--prompts", default=str(HERE / "prompts"))
    ap.add_argument("--restored", default=str(HERE / "restored"))
    ap.add_argument("--hand", default=str(HERE / "hand.json"))
    ap.add_argument("--json")
    args = ap.parse_args()
    cases = json.loads((HERE / "cases.json").read_text(encoding="utf-8"))
    people = {p: json.loads((HERE / "people" / f"{p}.json").read_text(encoding="utf-8"))["values"]
              for p in ("single", "family")}
    hand_path = Path(args.hand)
    hand = json.loads(hand_path.read_text(encoding="utf-8")) if hand_path.exists() else {}
    out = {}
    for model_dir in sorted(Path(args.answers).iterdir()):
        if not model_dir.is_dir():
            continue
        model = model_dir.name
        tot = dict(answers=0, r1_pass=0, r1_tokens=0, r1_bad=0, r2_pass=0, flags=0,
                   r3_pass=0, blanks=0, blanks_ok=0, facts=0, facts_ok=0)
        print(f"\n=== {model} ===")
        res = {}
        for ans in sorted(model_dir.glob("*.txt")):
            form_id, person = ans.stem.rsplit(".", 1)
            case = cases[form_id][person]
            values = people[person]
            text = ans.read_text(encoding="utf-8")
            prompt = (Path(args.prompts) / ans.name).read_text(encoding="utf-8")
            r1, r2, flags, r3, facts, nblanks, _ = score_answer(text, prompt, case, values)
            h = hand.get(f"{model}/{ans.stem}", {})
            hr3 = h.get("rule3", {})
            r3 = [x for x in r3 if not hr3.get(x[0], "").startswith("pass")]
            r3 += [(k, [], "hand " + v, "") for k, v in hr3.items()
                   if v.startswith("fail") and k not in [x[0] for x in r3]]
            if h.get("rule2", "").startswith("fail"):
                r2 = r2 + ["hand " + h["rule2"]]
            flags += [f"hand rule 3 {k}: {v}" for k, v in hr3.items()]
            if "rule2" in h:
                flags.append(f"hand rule 2: {h['rule2']}")
            ntok = len(TOKEN.findall(text))
            tot["answers"] += 1
            tot["r1_tokens"] += ntok
            tot["r1_bad"] += len(r1)
            tot["r1_pass"] += not r1
            tot["r2_pass"] += not r2
            tot["flags"] += len(flags)
            tot["blanks"] += nblanks
            tot["blanks_ok"] += nblanks - len(r3)
            tot["r3_pass"] += not r3
            nf = len(case.get("facts", {}))
            tot["facts"] += nf
            tot["facts_ok"] += nf - len(facts)
            res[ans.stem] = dict(r1=r1, r2=r2, flags=flags, r3=r3, facts=facts, tokens=ntok)
            mark = lambda ok: "pass" if ok else "FAIL"
            print(f"- {ans.stem}: tokens {ntok}; rule 1 {mark(not r1)}; rule 2 {mark(not r2)}"
                  f" ({len(flags)} flags); rule 3 {nblanks - len(r3)}/{nblanks}; facts {nf - len(facts)}/{nf}")
            for x in r1:
                print(f"    r1 {x}")
            for x in r2:
                print(f"    r2 {x}")
            for x in flags:
                print(f"    flag {x}")
            for key, need, why, chunk in r3:
                print(f"    r3 {key} {why}: {chunk!r}")
            for key, rx, chunk in facts:
                print(f"    fact {key} /{rx}/ not matched: {chunk!r}")
            rdir = Path(args.restored) / model
            rdir.mkdir(parents=True, exist_ok=True)
            (rdir / ans.name).write_text(restore(text, values), encoding="utf-8")
        print(f"TOTAL {model}: {json.dumps(tot)}")
        out[model] = dict(total=tot, answers=res)
    if args.json:
        Path(args.json).write_text(json.dumps(out, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
