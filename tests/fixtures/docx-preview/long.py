"""Write a synthetic contract of about 100 pages to the path given (not committed).

The phone measurement's long document (scripts/ios/ios-docx-measure.sh):
numbered clauses in English and Russian, a heading every few pages, a table
every ten, a comment every two and a footnote now and then, in Calibri and
Times New Roman, with a header and a page-number footer. Standard library
only, deterministic; no real document's text.
"""
import sys
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED

W = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main'
R = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
PAGES = 100
CLAUSES_PER_PAGE = 10  # about one rendered page each, in Calibri or Carlito

EN = ('The Supplier shall deliver the Goods to the Buyer at the Delivery Point within thirty days '
      'of each Order, and the risk in the Goods passes to the Buyer on delivery. Either Party may '
      'terminate this Agreement by written notice if the other Party commits a material breach '
      'that it does not remedy within fourteen days of being notified of it.')
RU = ('Поставщик передаёт Товар Покупателю в месте поставки в течение тридцати дней с даты каждого '
      'Заказа, и риск случайной гибели Товара переходит к Покупателю в момент передачи. Каждая из '
      'Сторон вправе расторгнуть Договор, письменно уведомив другую Сторону о существенном нарушении.')


def r(text, props=''):
    return f'<w:r>{props}<w:t xml:space="preserve">{text}</w:t></w:r>'


def body():
    out, comments, notes = [], [], []
    clause = 0
    for page in range(1, PAGES + 1):
        if page % 4 == 1:
            out.append(f'<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr>{r(f"Article {page // 4 + 1}. Terms of supply")}</w:p>')
        for i in range(CLAUSES_PER_PAGE):
            clause += 1
            text = EN if (clause % 3) else RU
            runs = r(f'{clause}. ', '<w:rPr><w:b/></w:rPr>') + r(text)
            if i == 2 and page % 2 == 0:
                cid = len(comments)
                comments.append((cid, 'Reviewer A' if cid % 2 else 'Рецензент Б',
                                 f'Check clause {clause}: the notice period differs from the master agreement.'))
                runs = (f'<w:commentRangeStart w:id="{cid}"/>' + runs + f'<w:commentRangeEnd w:id="{cid}"/>'
                        f'<w:r><w:commentReference w:id="{cid}"/></w:r>')
            if i == 4 and page % 7 == 0:
                nid = len(notes) + 1
                notes.append(nid)
                runs += f'<w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteReference w:id="{nid}"/></w:r>'
            out.append(f'<w:p><w:pPr><w:pStyle w:val="Clause"/></w:pPr>{runs}</w:p>')
        if page % 10 == 5:
            rows = ''.join(
                f'<w:tr><w:tc><w:p>{r(f"Item {n}")}</w:p></w:tc><w:tc><w:p>{r(f"{n * 125} units")}</w:p></w:tc>'
                f'<w:tc><w:p>{r(f"{n * 1500} RUB")}</w:p></w:tc></w:tr>' for n in range(1, 7))
            out.append('<w:tbl><w:tblPr><w:tblBorders>' + ''.join(
                f'<w:{side} w:val="single" w:sz="4"/>' for side in ['top', 'left', 'bottom', 'right', 'insideH', 'insideV'])
                + '</w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="3200"/><w:gridCol w:w="3200"/><w:gridCol w:w="3200"/></w:tblGrid>'
                + rows + '</w:tbl>')
    sect = ('<w:sectPr><w:headerReference w:type="default" r:id="header"/><w:footerReference w:type="default" r:id="footer"/>'
            '<w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440" w:header="720" w:footer="720"/></w:sectPr>')
    return ''.join(out) + sect, comments, notes


def p(text):
    return f'<w:p>{r(text)}</w:p>'


def rels(items):
    return ('<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            + ''.join(f'<Relationship Id="{i}" Type="{R}/{kind}" Target="{target}"/>' for i, kind, target in items)
            + '</Relationships>')


def main(path):
    text, comments, notes = body()
    page_field = ('<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r>'
                  '<w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>')
    parts = {
        '[Content_Types].xml': '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>' + ''.join(
            f'<Override PartName="/word/{name}.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.{kind}+xml"/>'
            for name, kind in [('document', 'document.main'), ('styles', 'styles'), ('comments', 'comments'),
                               ('footnotes', 'footnotes'), ('header1', 'header'), ('footer1', 'footer')]) + '</Types>',
        '_rels/.rels': rels([('document', 'officeDocument', 'word/document.xml')]),
        'word/_rels/document.xml.rels': rels([('styles', 'styles', 'styles.xml'), ('comments', 'comments', 'comments.xml'),
                                              ('notes', 'footnotes', 'footnotes.xml'), ('header', 'header', 'header1.xml'),
                                              ('footer', 'footer', 'footer1.xml')]),
        'word/document.xml': f'<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>{text}</w:body></w:document>',
        'word/styles.xml': f'<w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:cs="Calibri"/>'
        '<w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="259" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>'
        '<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:keepNext/><w:spacing w:before="360" w:after="120"/></w:pPr>'
        '<w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman"/><w:b/><w:sz w:val="28"/></w:rPr></w:style>'
        '<w:style w:type="paragraph" w:styleId="Clause"><w:name w:val="Clause"/><w:pPr><w:jc w:val="both"/><w:ind w:left="360"/></w:pPr></w:style></w:styles>',
        'word/comments.xml': f'<w:comments xmlns:w="{W}">' + ''.join(
            f'<w:comment w:id="{cid}" w:author="{author}" w:date="2026-11-18T10:00:00Z">{p(note)}</w:comment>'
            for cid, author, note in comments) + '</w:comments>',
        'word/footnotes.xml': f'<w:footnotes xmlns:w="{W}">' + ''.join(
            f'<w:footnote w:id="{nid}">{p(f"Footnote {nid}: as amended by the supplementary agreement.")}</w:footnote>'
            for nid in notes) + '</w:footnotes>',
        'word/header1.xml': f'<w:hdr xmlns:w="{W}">{p("Supply Agreement No. 100 — synthetic measurement document")}</w:hdr>',
        'word/footer1.xml': f'<w:ftr xmlns:w="{W}"><w:p><w:pPr><w:jc w:val="center"/></w:pPr>{r("Page ")}{page_field}</w:p></w:ftr>',
    }
    with ZipFile(path, 'w') as archive:
        for name, data in sorted(parts.items()):
            info = ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_DEFLATED
            archive.writestr(info, data.encode())


if __name__ == '__main__':
    main(sys.argv[1])
