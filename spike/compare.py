"""SPIKE (#903): compare raw model snapshots of one configuration against a baseline copy."""
import json, sys, pathlib
base, other = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
same = diff = 0; added = removed = 0; maxd = 0.0; n_ent = 0; diffs = []
for b in sorted(base.rglob("*.model.json")):
    o = other / b.relative_to(base)
    eb = {(e["start"], e["end"], e["label"]): e["score"] for e in json.loads(b.read_text())["entities"]}
    eo = {(e["start"], e["end"], e["label"]): e["score"] for e in json.loads(o.read_text())["entities"]}
    n_ent += len(eb)
    for k in eb.keys() & eo.keys():
        maxd = max(maxd, abs(eb[k] - eo[k]))
    if eb.keys() == eo.keys():
        same += 1
    else:
        diff += 1; added += len(eo.keys() - eb.keys()); removed += len(eb.keys() - eo.keys())
        near = [eb.get(k, eo.get(k)) for k in eb.keys() ^ eo.keys()]
        diffs.append((str(b.relative_to(base)), len(eo.keys() - eb.keys()), len(eb.keys() - eo.keys()), [round(s, 3) for s in near][:6]))
print(f"{other.name} vs {base.name}: fixtures identical {same}/{same+diff}; entities +{added} -{removed} of {n_ent}; max score delta on shared {maxd:.2e}")
for d in diffs[:15]:
    print("  ", d)
