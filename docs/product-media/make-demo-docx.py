"""Build the fictional demo DOCX from its Markdown, so the two never drift (python-docx).

Usage: python3 make-demo-docx.py ["Settlement agreement.md"]
Handles only what the demo uses: headings, paragraphs, numbered lists, one table,
a rule, and **bold** / *italic* runs.
"""
import re
import sys
from pathlib import Path

from docx import Document
from docx.enum.text import WD_ALIGN_PARAGRAPH
from docx.shared import Inches, Pt, RGBColor

INK = RGBColor.from_string('243E37')
RUN = re.compile(r'(\*\*[^*]+\*\*|\*[^*]+\*)')


def add_runs(paragraph, text):
    for part in RUN.split(text):
        if part.startswith('**'):
            paragraph.add_run(part[2:-2]).bold = True
        elif part.startswith('*'):
            paragraph.add_run(part[1:-1]).italic = True
        elif part:
            paragraph.add_run(part)
    return paragraph


def cells(line):
    return [c.strip() for c in line.strip().strip('|').split('|')]


def build(source: Path) -> Path:
    doc = Document()
    section = doc.sections[0]
    section.top_margin = section.bottom_margin = Inches(.75)
    section.left_margin = section.right_margin = Inches(.85)
    normal = doc.styles['Normal']
    normal.font.name = 'Calibri'
    normal.font.size = Pt(11)
    normal.paragraph_format.space_after = Pt(8)
    for name in ['Title', 'Heading 1']:
        doc.styles[name].font.name = 'Calibri'
        doc.styles[name].font.color.rgb = INK

    lines = source.read_text(encoding='utf-8').splitlines()
    i = 0
    while i < len(lines):
        line = lines[i].rstrip()
        if line.startswith('# '):
            doc.add_heading(line[2:], 0)
        elif line.startswith('## '):
            doc.add_heading(line[3:], 1)
        elif line.startswith('|'):
            rows = [cells(line)]
            while i + 1 < len(lines) and lines[i + 1].startswith('|'):
                i += 1
                if not set(lines[i]) <= set('|- '):
                    rows.append(cells(lines[i]))
            table = doc.add_table(rows=0, cols=len(rows[0]))
            table.style = 'Light Shading Accent 1'
            for row in rows:
                for cell, value in zip(table.add_row().cells, row):
                    cell.text = value
        elif line == '---':
            footer = section.footer.paragraphs[0]
            footer.text = 'FICTIONAL AGREEMENT  ·  PRODUCT DEMONSTRATION'
            footer.alignment = WD_ALIGN_PARAGRAPH.CENTER
            footer.runs[0].font.size = Pt(8)
        elif m := re.match(r'(\d+\.) (.*)', line):
            # Literal numbers: python-docx's List Number continues across lists.
            item = doc.add_paragraph()
            item.paragraph_format.left_indent = Inches(.3)
            item.paragraph_format.first_line_indent = Inches(-.3)
            item.add_run(f'{m.group(1)}\t')
            add_runs(item, m.group(2))
        elif line:
            add_runs(doc.add_paragraph(), line)
        i += 1

    doc.core_properties.author = 'Lawkitt'
    doc.core_properties.title = f'{source.stem} — fictional demonstration'
    target = source.with_suffix('.docx')
    doc.save(target)
    return target


if __name__ == '__main__':
    here = Path(__file__).parent
    print(build(here / (sys.argv[1] if len(sys.argv) > 1 else 'Settlement agreement.md')))
