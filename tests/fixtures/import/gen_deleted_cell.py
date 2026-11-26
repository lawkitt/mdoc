"""Generate handmade-deleted-cell.docx: a synthetic DOCX whose table has a
cell deleted as a tracked change (w:cellDel) holding a made-up person's
name and phone number, which must not reach the Markdown (#395). Word does
not track cell deletions; other programs write them. Deterministic; no
Office needed. Run: python gen_deleted_cell.py
"""
from pathlib import Path
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED

W = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main'
PKG = 'http://schemas.openxmlformats.org/package/2006/relationships'
DOC_RELS = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
CHANGE = 'w:author="Reviewer" w:date="2026-11-27T09:00:00Z"'


def cell(text, mark=''):
    return (f'<w:tc><w:tcPr><w:tcW w:w="2400" w:type="dxa"/>{mark}</w:tcPr>'
            f'<w:p><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p></w:tc>')


def deleted(id):
    return f'<w:cellDel w:id="{id}" {CHANGE}/>'


def inserted(id):
    return f'<w:cellIns w:id="{id}" {CHANGE}/>'


body = ''.join([
    '<w:p><w:r><w:t>The parties to the lease.</w:t></w:r></w:p>',
    '<w:tbl><w:tblPr/><w:tblGrid>',
    '<w:gridCol w:w="2400"/><w:gridCol w:w="2400"/><w:gridCol w:w="2400"/></w:tblGrid>',
    # The middle column was deleted cell by cell; an inserted cell stays.
    '<w:tr>', cell('Party'), cell('Guarantor', deleted(1)), cell('Role'), '</w:tr>',
    '<w:tr>', cell('Landlord Ltd'), cell('Marisol Quenby', deleted(2)), cell('Owner'), '</w:tr>',
    '<w:tr>', cell('The Tenant'), cell('Call 555-0173', deleted(3)), cell('Occupier', inserted(4)),
    '</w:tr></w:tbl>',
    '<w:p><w:r><w:t>End of the schedule.</w:t></w:r></w:p>',
])

parts = {
    '_rels/.rels': (f'<Relationships xmlns="{PKG}"><Relationship Id="document" '
                    f'Type="{DOC_RELS}/officeDocument" Target="word/document.xml"/></Relationships>'),
    'word/document.xml': f'<w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>',
    '[Content_Types].xml': (
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/word/document.xml" '
        'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
        '</Types>'
    ),
}

with ZipFile(Path(__file__).with_name('handmade-deleted-cell.docx'), 'w') as archive:
    for name, data in sorted(parts.items()):
        info = ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
        info.compress_type = ZIP_DEFLATED
        archive.writestr(info, data.encode())
