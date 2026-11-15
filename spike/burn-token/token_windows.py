"""SPIKE (#903): windows for burn-token, and ORT-vs-Burn comparison on the public fixtures.

  python token_windows.py dump <model-id> <onnx> <windows.json> [fixtures|sample-en|sample-ru]
  python token_windows.py compare <model-id> <onnx> <windows.json> <burn.bin>
Run from tools/detection-eval/ of the harness (#904) with its venv.
"""
import json, sys
import numpy as np


def texts(which):
    from detection_eval import fixtures, time_onnx
    if which.startswith("sample-"):
        return [time_onnx.sample(which.split("-")[1])]
    out = []
    for lang in ("en", "ru"):
        for p in fixtures.fixture_paths(fixtures.sets()["fixtures"] / lang):
            out.append(fixtures.source_of(p))
    return out


def main():
    from detection_eval.adapters import load_onnx, decode_tags
    from detection_eval.models import BY_ID
    from detection_eval.windows import bounded_windows
    cmd, mid, onnx, wpath = sys.argv[1:5]
    model = BY_ID[mid]
    ad = load_onnx(model, onnx, 4)
    if cmd == "dump":
        ws = []
        for text in texts(sys.argv[5] if len(sys.argv) > 5 else "fixtures"):
            for s, e in bounded_windows(text, ad.fits):
                win = text[s:e]
                enc = ad.tokenizer(win, return_offsets_mapping=True, truncation=True, max_length=ad.limit)
                ws.append({"text": win, "input_ids": enc["input_ids"], "attention_mask": enc["attention_mask"],
                           "offsets": enc["offset_mapping"]})
        json.dump(ws, open(wpath, "w"))
        print(len(ws), "windows,", sum(len(w["input_ids"]) for w in ws), "tokens")
        return
    ws = json.load(open(wpath))
    burn = np.fromfile(sys.argv[5], dtype=np.float32)
    nl = len(ad.id2label)
    pos = 0; tok_agree = tok = 0; maxd = 0.0; same_ents = 0; diff_ents = 0
    for w in ws:
        n = len(w["input_ids"])
        b = burn[pos:pos + n * nl].reshape(n, nl); pos += n * nl
        feed = {"input_ids": np.array([w["input_ids"]], np.int64), "attention_mask": np.array([w["attention_mask"]], np.int64)}
        feed = {k: v for k, v in feed.items() if k in ad.inputs}
        r = ad.session.run(None, feed)[0][0].astype(np.float32)
        maxd = max(maxd, float(np.abs(r - b).max()))
        tok += n; tok_agree += int((r.argmax(-1) == b.argmax(-1)).sum())
        def ents(lg):
            lg = lg - lg.max(-1, keepdims=True); p = np.exp(lg); p /= p.sum(-1, keepdims=True)
            return sorted((e.start, e.end, e.label) for e in decode_tags(w["text"], w["offsets"], p, ad.id2label, model.labels, model.cut, model.decoder))
        if ents(r) == ents(b):
            same_ents += 1
        else:
            diff_ents += 1
    print(json.dumps({"windows": len(ws), "tokens": tok, "argmax_agree": tok_agree, "max_abs_logit_diff": round(maxd, 5),
                      "windows_same_entities": same_ents, "windows_diff_entities": diff_ents}))


main()
