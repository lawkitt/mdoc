"""Generate a deterministic, synthetic DOCX qualification fixture (no Office needed)."""
from pathlib import Path
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED

W = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main'
R = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'

def p(text):
    return f'<w:p><w:r><w:t>{text}</w:t></w:r></w:p>'

def rels(items):
    return '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' + ''.join(
        f'<Relationship Id="{id}" Type="{R}/{kind}" Target="{target}"{external}/>'
        for id, kind, target, external in items) + '</Relationships>'

body = '<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Qualification heading</w:t></w:r></w:p>'
body += p('Latin text and Cyrillic: Привет мир. Проверка поиска.')
body += '<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2400"/><w:gridCol w:w="2400"/></w:tblGrid>'
body += '<w:tr><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr>' + p('Merged horizontal') + '</w:tc></w:tr>'
body += '<w:tr><w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr>' + p('Merged vertical') + '</w:tc><w:tc>' + p('Cell alpha') + '</w:tc></w:tr>'
body += '<w:tr><w:tc><w:tcPr><w:vMerge/></w:tcPr>' + p('') + '</w:tc><w:tc>' + p('Cell beta') + '</w:tc></w:tr></w:tbl>'
for level in [0, 1]:
    body += f'<w:p><w:pPr><w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>List level {level}</w:t></w:r></w:p>'
body += '<w:p><w:hyperlink r:id="link"><w:r><w:t>Example hyperlink</w:t></w:r></w:hyperlink><w:r><w:footnoteReference w:id="1"/></w:r></w:p>'
body += '''<w:p><w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="914400"/><wp:docPr id="1" name="Red square"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="Red square"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="image"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>'''
for page in range(2, 10):
    body += '<w:p><w:r><w:br w:type="page"/></w:r></w:p>'
    body += p(f'Page marker {page}')
    for line in range(18):
        body += p(f'Paragraph {line}: The quick brown fox. Проверка текста и поиска на странице {page}.')
portrait = '<w:headerReference w:type="default" r:id="header"/><w:footerReference w:type="default" r:id="footer"/><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440" w:header="720" w:footer="720"/>'
body += '<w:p><w:pPr><w:sectPr>' + portrait + '</w:sectPr></w:pPr></w:p>'
body += p('Landscape final section')
body += '<w:sectPr><w:type w:val="nextPage"/><w:pgSz w:w="15840" w:h="12240" w:orient="landscape"/><w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440"/></w:sectPr>'
parts = {
    '_rels/.rels': rels([('document','officeDocument','word/document.xml','')]),
    'word/document.xml': f'<w:document xmlns:w="{W}" xmlns:r="{R}" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body>{body}</w:body></w:document>',
    'word/_rels/document.xml.rels': rels([('styles','styles','styles.xml',''),('numbering','numbering','numbering.xml',''),('header','header','header1.xml',''),('footer','footer','footer1.xml',''),('notes','footnotes','footnotes.xml',''),('image','image','media/red.png',''),('link','hyperlink','https://example.com',' TargetMode="External"')]),
    'word/styles.xml': f'<w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style></w:styles>',
    'word/numbering.xml': f'<w:numbering xmlns:w="{W}"><w:abstractNum w:abstractNumId="0">' + ''.join(f'<w:lvl w:ilvl="{i}"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%{i+1}."/></w:lvl>' for i in [0,1]) + '</w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>',
    'word/header1.xml': f'<w:hdr xmlns:w="{W}">{p("Qualification header")}</w:hdr>',
    'word/footer1.xml': f'<w:ftr xmlns:w="{W}">{p("Qualification footer")}</w:ftr>',
    'word/footnotes.xml': f'<w:footnotes xmlns:w="{W}"><w:footnote w:id="1">{p("Footnote content")}</w:footnote></w:footnotes>',
}
content_types = {'document':'document.main','styles':'styles','numbering':'numbering','header1':'header','footer1':'footer','footnotes':'footnotes'}
parts['[Content_Types].xml'] = '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="png" ContentType="image/png"/>' + ''.join(f'<Override PartName="/word/{name}.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.{kind}+xml"/>' for name,kind in content_types.items()) + '</Types>'
# 1x1 red PNG, generated without image-library dependencies.
import struct, zlib
chunk = lambda kind, data: struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
parts['word/media/red.png'] = b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',1,1,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b'\x00\xff\x00\x00'))+chunk(b'IEND',b'')
with ZipFile(Path(__file__).with_name('coverage.docx'),'w') as archive:
    for name, data in sorted(parts.items()):
        info = ZipInfo(name, date_time=(2026,1,1,0,0,0))
        info.compress_type = ZIP_DEFLATED
        archive.writestr(info, data.encode() if isinstance(data,str) else data)

# Separate small comments fixture: ranges across paragraphs, overlap, a point
# comment, and an unanchored comment. Keep the coverage document unchanged.
comment_parts = dict(parts)
comment_parts['word/document.xml'] = f'''<w:document xmlns:w="{W}"><w:body>
<w:p><w:commentRangeStart w:id="0"/><w:r><w:t>First quoted paragraph.</w:t></w:r></w:p>
<w:p><w:commentRangeStart w:id="1"/><w:r><w:t>Second quoted paragraph.</w:t></w:r><w:commentRangeEnd w:id="1"/><w:commentRangeEnd w:id="0"/><w:r><w:commentReference w:id="0"/><w:commentReference w:id="1"/></w:r></w:p>
<w:p><w:r><w:t>Point comment context.</w:t><w:commentReference w:id="2"/></w:r></w:p>
<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440"/></w:sectPr>
</w:body></w:document>'''
comment_parts['word/comments.xml'] = f'''<w:comments xmlns:w="{W}">
<w:comment w:id="0" w:author="Reviewer" w:date="2026-09-19T10:00:00Z">{p('Please check this clause.')}{p('Second line: &amp; &lt;literal&gt;.')}</w:comment>
<w:comment w:id="1" w:author="Ирина">{p('Проверить условия.')}</w:comment>
<w:comment w:id="2" w:author="Point reviewer">{p('Point note.')}</w:comment>
<w:comment w:id="3">{p('Unanchored note is still visible.')}</w:comment>
</w:comments>'''
comment_parts['word/_rels/document.xml.rels'] = rels([('styles','styles','styles.xml',''),('comments','comments','comments.xml','')])
comment_parts['[Content_Types].xml'] = comment_parts['[Content_Types].xml'].replace('</Types>', '<Override PartName="/word/comments.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"/></Types>')
with ZipFile(Path(__file__).with_name('comments.docx'),'w') as archive:
    for name, data in sorted(comment_parts.items()):
        info = ZipInfo(name, date_time=(2026,1,1,0,0,0))
        info.compress_type = ZIP_DEFLATED
        archive.writestr(info, data.encode() if isinstance(data,str) else data)
