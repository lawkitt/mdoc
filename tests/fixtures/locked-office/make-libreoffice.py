"""Writes the LibreOffice fixtures in this folder through LibreOffice itself.

Run with LibreOffice's own Python while soffice listens headless:

    soffice --headless --invisible --norestore \
        "--accept=socket,host=127.0.0.1,port=2002;urp;"
    "C:\\Program Files\\LibreOffice\\program\\python.exe" \
        tests/fixtures/locked-office/make-libreoffice.py

The text is fictional. The password is a published test value (README.md).
"""

import os
import time

import uno
from com.sun.star.beans import PropertyValue

HERE = os.path.dirname(os.path.abspath(__file__))
PASSWORD = "Petrov-2026"
LINES = [
    "Engagement letter",
    "Client: Ivan Petrov, 12 Baker Street, Springfield.",
    "Matter: lease dispute with Northwind Holdings Ltd.",
    "Клиент: Иван Петров, ул. Садовая, 12.",
]
ROWS = [
    ("Client", "Ivan Petrov"),
    ("Address", "12 Baker Street, Springfield"),
    ("Matter", "Lease dispute with Northwind Holdings Ltd."),
    ("Fee", 1200),
]


def prop(name, value):
    p = PropertyValue()
    p.Name = name
    p.Value = value
    return p


def connect():
    local = uno.getComponentContext()
    resolver = local.ServiceManager.createInstanceWithContext("com.sun.star.bridge.UnoUrlResolver", local)
    for _ in range(60):
        try:
            ctx = resolver.resolve("uno:socket,host=127.0.0.1,port=2002;urp;StarOffice.ComponentContext")
            return ctx.ServiceManager.createInstanceWithContext("com.sun.star.frame.Desktop", ctx)
        except Exception:  # soffice still starting
            time.sleep(1)
    raise SystemExit("soffice is not listening on port 2002")


def new(desktop, kind):
    return desktop.loadComponentFromURL(f"private:factory/{kind}", "_blank", 0, (prop("Hidden", True),))


def save(doc, name, filter_name, locked):
    path = os.path.join(HERE, name)
    if os.path.exists(path):
        os.remove(path)
    args = [prop("FilterName", filter_name), prop("Overwrite", True)]
    if locked:
        args.append(prop("Password", PASSWORD))
    doc.storeToURL(uno.systemPathToFileUrl(path), tuple(args))


def writer(desktop):
    doc = new(desktop, "swriter")
    text = doc.Text
    for i, line in enumerate(LINES):
        if i:
            text.insertControlCharacter(text.End, 0, False)  # PARAGRAPH_BREAK
        text.insertString(text.End, line, False)
    for name, filter_name, locked in [
        ("libreoffice-locked.docx", "MS Word 2007 XML", True),
        ("libreoffice-open.docx", "MS Word 2007 XML", False),
        ("libreoffice-locked.doc", "MS Word 97", True),
        ("libreoffice-open.doc", "MS Word 97", False),
    ]:
        save(doc, name, filter_name, locked)
    doc.close(True)


def calc(desktop):
    doc = new(desktop, "scalc")
    sheet = doc.Sheets.getByIndex(0)
    for r, (label, value) in enumerate(ROWS):
        sheet.getCellByPosition(0, r).String = label
        if isinstance(value, str):
            sheet.getCellByPosition(1, r).String = value
        else:
            sheet.getCellByPosition(1, r).Value = value
    for name, locked in [("libreoffice-locked.xlsx", True), ("libreoffice-open.xlsx", False)]:
        save(doc, name, "Calc MS Excel 2007 XML", locked)
    doc.close(True)


def impress(desktop):
    doc = new(desktop, "simpress")
    page = doc.DrawPages.getByIndex(0)
    # The first page has the title-and-content layout's two placeholders.
    page.Layout = 1  # AUTOLAYOUT_ENUM: title + outline
    shapes = [page.getByIndex(i) for i in range(page.Count)]
    shapes[0].String = LINES[0]
    shapes[1].String = "\n".join(LINES[1:])
    for name, locked in [("libreoffice-locked.pptx", True), ("libreoffice-open.pptx", False)]:
        save(doc, name, "Impress MS PowerPoint 2007 XML", locked)
    doc.close(True)


def main():
    desktop = connect()
    writer(desktop)
    calc(desktop)
    impress(desktop)
    for name in sorted(os.listdir(HERE)):
        if name.startswith("libreoffice-"):
            print(f"{name:28} {os.path.getsize(os.path.join(HERE, name))}")


main()
