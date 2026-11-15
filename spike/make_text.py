import re, sys, pathlib
root, lang, out = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
docs = []
for p in sorted((root / lang).rglob("*.md")):
    if p.parent.name in ("tables", "ocr", "docx"):
        continue
    s = p.read_text()
    if s.startswith("---"):
        s = s.split("---", 2)[2]
    s = re.sub(r"⟦[^|⟧]*\|([^⟧]*)⟧", r"\1", s).strip()
    docs.append(s)
docs.sort(key=len, reverse=True)
text = ""
for d in docs:
    if len(text) >= 10000:
        break
    text += d + "\n\n"
cut = text[:10000]
cut = cut[: cut.rfind(" ")] if len(text) > 10000 else cut
pathlib.Path(out).write_text(cut)
print(lang, len(cut), "chars")
