"""Generate handmade-hidden-content.docx: a synthetic DOCX whose comments,
deleted tracked changes and document properties name people who must not
reach the Markdown (ROADMAP 0.4.0 item 6). Deterministic; no Office needed.
Run: python gen_hidden_content.py
"""
from pathlib import Path
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED

W = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main'
R = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
PKG = 'http://schemas.openxmlformats.org/package/2006/relationships'
DOC_RELS = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'


def run(text):
    return f'<w:r><w:t xml:space="preserve">{text}</w:t></w:r>'


def deleted(text, id):
    return (f'<w:del w:id="{id}" w:author="Ivan Petrov" w:date="2026-10-01T09:00:00Z">'
            f'<w:r><w:delText xml:space="preserve">{text}</w:delText></w:r></w:del>')


def inserted(text, id):
    return (f'<w:ins w:id="{id}" w:author="Ivan Petrov" w:date="2026-10-01T09:00:00Z">'
            f'{run(text)}</w:ins>')


def rels(items):
    return f'<Relationships xmlns="{PKG}">' + ''.join(
        f'<Relationship Id="{id}" Type="{kind}" Target="{target}"/>'
        for id, kind, target in items) + '</Relationships>'


body = ''.join([
    # A comment on the opening sentence, and a deletion replaced by an insertion.
    '<w:p><w:commentRangeStart w:id="0"/>', run('The tenant is '), deleted('Margaret Holloway', 1),
    inserted('the Client', 2), run('.'), '<w:commentRangeEnd w:id="0"/>',
    '<w:r><w:commentReference w:id="0"/></w:r></w:p>',
    # A deleted clause inside a hyperlink-free paragraph, and a moved sentence.
    '<w:p>', run('The deposit is due on signing.'), deleted(' Call 555-0142 first.', 3),
    '<w:r><w:commentReference w:id="1"/></w:r></w:p>',
    '<w:p><w:moveFrom w:id="4" w:author="Ivan Petrov" w:date="2026-10-01T09:00:00Z">',
    '<w:r><w:t>Moved sentence.</w:t></w:r></w:moveFrom></w:p>',
    '<w:p><w:moveTo w:id="5" w:author="Ivan Petrov" w:date="2026-10-01T09:00:00Z">',
    run('Moved sentence.'), '</w:moveTo></w:p>',
    # A table cell with a deleted name.
    '<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2400"/><w:gridCol w:w="2400"/></w:tblGrid>',
    '<w:tr><w:tc><w:p>', run('Party'), '</w:p></w:tc><w:tc><w:p>', run('Role'), '</w:p></w:tc></w:tr>',
    '<w:tr><w:tc><w:p>', deleted('Holloway Trust', 6), inserted('Landlord Ltd', 7),
    '</w:p></w:tc><w:tc><w:p>', run('Landlord'), '</w:p></w:tc></w:tr></w:tbl>',
    # A footnote with a deletion.
    '<w:p>', run('See the note.'), '<w:r><w:footnoteReference w:id="1"/></w:r></w:p>',
])

comments = (
    f'<w:comments xmlns:w="{W}">'
    '<w:comment w:id="0" w:author="Ivan Petrov" w:initials="IP" w:date="2026-10-01T09:00:00Z">'
    '<w:p><w:r><w:t>Ask Margaret Holloway about the guarantor.</w:t></w:r></w:p></w:comment>'
    '<w:comment w:id="1" w:author="Ivan Petrov" w:initials="IP">'
    '<w:p><w:r><w:t>Deposit came from Holloway Trust.</w:t></w:r></w:p></w:comment>'
    '</w:comments>'
)

footnotes = (
    f'<w:footnotes xmlns:w="{W}"><w:footnote w:id="1"><w:p>'
    + run('Payable to the landlord') + deleted(' via Margaret Holloway', 8) + run('.')
    + '</w:p></w:footnote></w:footnotes>'
)

core = (
    '<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
    'xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/">'
    '<dc:title>Holloway lease</dc:title><dc:subject>Margaret Holloway tenancy</dc:subject>'
    '<dc:creator>Margaret Holloway</dc:creator><cp:lastModifiedBy>Ivan Petrov</cp:lastModifiedBy>'
    '<cp:keywords>Holloway</cp:keywords><dc:description>Draft for Margaret Holloway</dc:description>'
    '</cp:coreProperties>'
)
app = ('<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties">'
       '<Company>Holloway Trust</Company><Manager>Ivan Petrov</Manager></Properties>')
custom = ('<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" '
          'xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">'
          '<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2" name="Client">'
          '<vt:lpwstr>Margaret Holloway</vt:lpwstr></property></Properties>')

parts = {
    '_rels/.rels': rels([
        ('document', f'{DOC_RELS}/officeDocument', 'word/document.xml'),
        ('core', 'http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties', 'docProps/core.xml'),
        ('app', f'{DOC_RELS}/extended-properties', 'docProps/app.xml'),
        ('custom', f'{DOC_RELS}/custom-properties', 'docProps/custom.xml'),
    ]),
    'word/document.xml': f'<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>{body}</w:body></w:document>',
    'word/_rels/document.xml.rels': rels([
        ('comments', f'{DOC_RELS}/comments', 'comments.xml'),
        ('notes', f'{DOC_RELS}/footnotes', 'footnotes.xml'),
    ]),
    'word/comments.xml': comments,
    'word/footnotes.xml': footnotes,
    'docProps/core.xml': core,
    'docProps/app.xml': app,
    'docProps/custom.xml': custom,
}
overrides = {
    '/word/document.xml': 'application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml',
    '/word/comments.xml': 'application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml',
    '/word/footnotes.xml': 'application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml',
    '/docProps/core.xml': 'application/vnd.openxmlformats-package.core-properties+xml',
    '/docProps/app.xml': 'application/vnd.openxmlformats-officedocument.extended-properties+xml',
    '/docProps/custom.xml': 'application/vnd.openxmlformats-officedocument.custom-properties+xml',
}
parts['[Content_Types].xml'] = (
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="xml" ContentType="application/xml"/>'
    + ''.join(f'<Override PartName="{name}" ContentType="{kind}"/>' for name, kind in overrides.items())
    + '</Types>'
)

with ZipFile(Path(__file__).with_name('handmade-hidden-content.docx'), 'w') as archive:
    for name, data in sorted(parts.items()):
        info = ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
        info.compress_type = ZIP_DEFLATED
        archive.writestr(info, data.encode())
