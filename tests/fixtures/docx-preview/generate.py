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
# comment, an unanchored comment, a reply, a resolved thread (Word 2012's
# commentsExtended part) and a passage of hidden text. Keep the coverage
# document unchanged.
W14 = 'http://schemas.microsoft.com/office/word/2010/wordml'
W15 = 'http://schemas.microsoft.com/office/word/2012/wordml'
def cp(text, para):
    return f'<w:p w14:paraId="{para}"><w:r><w:t>{text}</w:t></w:r></w:p>'
comment_parts = dict(parts)
comment_parts['word/document.xml'] = f'''<w:document xmlns:w="{W}"><w:body>
<w:p><w:commentRangeStart w:id="0"/><w:commentRangeStart w:id="5"/><w:r><w:t>First quoted paragraph.</w:t></w:r></w:p>
<w:p><w:commentRangeStart w:id="1"/><w:r><w:t>Second quoted paragraph.</w:t></w:r><w:commentRangeEnd w:id="1"/><w:commentRangeEnd w:id="0"/><w:commentRangeEnd w:id="5"/><w:r><w:commentReference w:id="0"/><w:commentReference w:id="1"/><w:commentReference w:id="5"/></w:r></w:p>
<w:p><w:r><w:t>Point comment context.</w:t><w:commentReference w:id="2"/></w:r></w:p>
<w:p><w:commentRangeStart w:id="4"/><w:r><w:t>A settled clause.</w:t></w:r><w:commentRangeEnd w:id="4"/><w:r><w:commentReference w:id="4"/></w:r><w:r><w:t xml:space="preserve"> Visible text</w:t></w:r><w:r><w:rPr><w:vanish/></w:rPr><w:t xml:space="preserve"> with a hidden drafting note</w:t></w:r><w:r><w:t>.</w:t></w:r></w:p>
<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440"/></w:sectPr>
</w:body></w:document>'''
comment_parts['word/comments.xml'] = f'''<w:comments xmlns:w="{W}" xmlns:w14="{W14}">
<w:comment w:id="0" w:author="Reviewer" w:date="2026-09-19T10:00:00Z">{p('Please check this clause.')}{cp('Second line: &amp; &lt;literal&gt;.', '00000001')}</w:comment>
<w:comment w:id="1" w:author="Ирина">{cp('Проверить условия.', '00000002')}</w:comment>
<w:comment w:id="2" w:author="Point reviewer">{cp('Point note.', '00000003')}</w:comment>
<w:comment w:id="3">{cp('Unanchored note is still visible.', '00000004')}</w:comment>
<w:comment w:id="4" w:author="Counsel" w:date="2026-09-20T09:30:00Z">{cp('Agreed with the other side.', '00000005')}</w:comment>
<w:comment w:id="5" w:author="Associate" w:date="2026-09-19T12:15:00Z">{cp('Checked: it matches the term sheet.', '00000006')}</w:comment>
</w:comments>'''
comment_parts['word/commentsExtended.xml'] = f'''<w15:commentsEx xmlns:w15="{W15}">
<w15:commentEx w15:paraId="00000001" w15:done="0"/>
<w15:commentEx w15:paraId="00000005" w15:done="1"/>
<w15:commentEx w15:paraId="00000006" w15:paraIdParent="00000001" w15:done="0"/>
</w15:commentsEx>'''
comment_parts['word/_rels/document.xml.rels'] = rels([('styles','styles','styles.xml',''),('comments','comments','comments.xml','')]).replace('</Relationships>', '<Relationship Id="commentsEx" Type="http://schemas.microsoft.com/office/2011/relationships/commentsExtended" Target="commentsExtended.xml"/></Relationships>')
comment_parts['[Content_Types].xml'] = comment_parts['[Content_Types].xml'].replace('</Types>', '<Override PartName="/word/comments.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"/><Override PartName="/word/commentsExtended.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml"/></Types>')
with ZipFile(Path(__file__).with_name('comments.docx'),'w') as archive:
    for name, data in sorted(comment_parts.items()):
        info = ZipInfo(name, date_time=(2026,1,1,0,0,0))
        info.compress_type = ZIP_DEFLATED
        archive.writestr(info, data.encode() if isinstance(data,str) else data)

# Comment anchors (ROADMAP Comments item 1): every range shape the render
# copy turns into `mdoc-comment:` links. Ids name the case in the tests.
def cs(id): return f'<w:commentRangeStart w:id="{id}"/>'
def ce(id): return f'<w:commentRangeEnd w:id="{id}"/><w:r><w:commentReference w:id="{id}"/></w:r>'
def r(text): return f'<w:r><w:t xml:space="preserve">{text}</w:t></w:r>'
long = ('This clause runs over several lines so that its comment covers more than one line of the page: '
        'the supplier delivers the goods to the buyer at the agreed place within thirty days of the order, '
        'and the risk passes to the buyer on delivery.')
anchors_body = f'''<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr>{r('Comment anchors')}</w:p>
<w:p>{cs(0)}{r(long)}{ce(0)}</w:p>
<w:p>{cs(1)}{r('First paragraph of a range over two paragraphs.')}</w:p>
<w:p>{r('Second paragraph of that range.')}{ce(1)}</w:p>
<w:p>{cs(2)}{r('Alpha beta ')}{cs(3)}{r('gamma')}{ce(2)}{r(' delta')}{ce(3)}{r(' epsilon.')}</w:p>
<w:p>{r('See ')}{cs(4)}{r('the ')}<w:hyperlink r:id="register">{r('public register')}</w:hyperlink>{ce(4)}{r(' today.')}</w:p>
<w:p>{r('Inside a link: ')}<w:hyperlink r:id="register">{r('the ')}{cs(11)}{r('register')}{ce(11)}</w:hyperlink>{r('.')}</w:p>
<w:p>{r('Point comment here')}<w:r><w:commentReference w:id="5"/></w:r>{r(' and the rest of the line.')}</w:p>
<w:p>{r('Empty')}{cs(6)}{ce(6)}{r(' range comment.')}</w:p>
<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="4800"/><w:gridCol w:w="4800"/></w:tblGrid><w:tr><w:tc><w:p>{r('Cell one')}</w:p></w:tc><w:tc><w:p>{cs(7)}{r('Commented cell')}{ce(7)}</w:p></w:tc></w:tr></w:tbl>
<w:p>{r('Русский текст: ')}{cs(8)}{r('Срок оплаты — тридцать дней.')}{ce(8)}</w:p>
<w:p>{r('As agreed in ')}{cs(9)}<w:hyperlink w:anchor="clause5">{r('clause 5')}</w:hyperlink>{ce(9)}{r('.')}</w:p>
<w:p>{cs(12)}{r('Before the page break.')}</w:p>
<w:p><w:r><w:br w:type="page"/></w:r></w:p>
<w:p>{r('After the page break.')}{ce(12)}</w:p>
<w:p><w:bookmarkStart w:id="0" w:name="clause5"/>{r('Clause 5. Payment.')}<w:bookmarkEnd w:id="0"/></w:p>
<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440"/></w:sectPr>'''
anchor_parts = dict(parts)
anchor_parts['word/document.xml'] = f'<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>{anchors_body}</w:body></w:document>'
anchor_parts['word/comments.xml'] = f'<w:comments xmlns:w="{W}">' + ''.join(
    f'<w:comment w:id="{id}" w:author="Reviewer" w:date="2026-11-18T10:00:00Z">{p(text)}</w:comment>'
    for id, text in [(0, 'Several lines.'), (1, 'Two paragraphs.'), (2, 'Overlaps the next.'), (3, 'Overlapped.'),
                     (4, 'Over a link.'), (5, 'Point.'), (6, 'Empty range.'), (7, 'In a cell.'), (8, 'По-русски.'),
                     (9, 'Over a cross-reference.'), (10, 'Nowhere.'), (11, 'Inside a link.'), (12, 'Across pages.')]) + '</w:comments>'
anchor_parts['word/_rels/document.xml.rels'] = rels([('styles','styles','styles.xml',''),('comments','comments','comments.xml',''),('register','hyperlink','https://example.com/register',' TargetMode="External"')])
anchor_parts['[Content_Types].xml'] = comment_parts['[Content_Types].xml']
with ZipFile(Path(__file__).with_name('comment-anchors.docx'),'w') as archive:
    for name, data in sorted(anchor_parts.items()):
        info = ZipInfo(name, date_time=(2026,1,1,0,0,0))
        info.compress_type = ZIP_DEFLATED
        archive.writestr(info, data.encode() if isinstance(data,str) else data)
