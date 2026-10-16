"""Scrub a private DOCX into a distributable layout fixture (standard library only).

Usage: python3 -I tests/fixtures/docx-preview/scrub.py <private.docx> [out.docx]

Keeps the package structure, styles, numbering, tables, sections, footnotes,
header/footer and every run's formatting and character count. Every word of
three or more letters that is not a function word becomes a dictionary word of
the same length, chosen by its position (part, paragraph, word) rather than
derived from the original, so the text cannot be reconstructed. Digits are
replaced the same way. Images become blank PNGs of the same size; author,
Grammarly and alt-text metadata and external link targets are neutralized.
"""
import hashlib
import html
import re
import struct
import sys
import zlib
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

KEEP = set('''a an and are as at be been but by can could did do does for from had has
have he her his how if in into is it its may might must no nor not of on or our shall
she should so such than that the their them then there these they this those to under
upon us was we were what when where which while who whom why will with within without
would you your'''.split())

VOCAB = '''act fee tax law due day sum net use pay own set key aim end way
fund term firm deed land rate cost date sale loan bond debt risk rule plan part form
file list note case fact item unit rank role seal vote
asset trust claim court party offer price share stock title value right order entry
scope basis cause issue lease board audit clerk grant
notice matter holder budget policy merger report review branch client broker credit
member option period record refund region tenant vendor sector market office permit agency
account auditor balance company consent counsel default deposit dispute finance licence
measure payment premium product receipt service statute subject summary trustee warrant
witness version
contract covenant director document evidence guidance interest judgment majority minority
ordinary schedule security standard transfer tribunal warranty location property purchase
approval
agreement authority condition liability guarantee signatory statement procedure provision
reference principal operation ownership insurance quarterly threshold committee recipient
obligation settlement regulation commission accounting assignment compliance consultant
membership investment governance instrument enterprise beneficial collateral procedural
arrangement shareholder beneficiary partnership certificate termination transaction
undertaking information contractual appointment competition enforcement independent
legislation
jurisdiction consolidated distribution registration requirements construction commencement
confidential notification organization subscription verification
incorporation documentation establishment international authorization determination
consideration participation organizations circumstances extraordinary unconditional
administrative accountability implementation representative responsibility recommendation
interpretation qualifications correspondence characteristic
notwithstanding confidentiality representatives recommendations internationally
interpretations
responsibilities disproportionate'''.split()
BY_LENGTH = {}
for word in VOCAB:
    BY_LENGTH.setdefault(len(word), []).append(word)

WORD = re.compile(r'[A-Za-zÀ-ɏЀ-ӿ]+|[0-9]')
PARAGRAPH = re.compile(r'<w:p[ >].*?</w:p>', re.S)
TEXT = re.compile(r'(<w:t(?: [^>]*)?>)([^<]*)(</w:t>)')


def pick(key, choices):
    digest = hashlib.sha256(key.encode()).digest()
    return choices[int.from_bytes(digest[:4], 'big') % len(choices)]


def replace_word(word, key):
    if word.isdigit():
        return pick(key, '0123456789')
    if len(word) < 3 or word.lower() in KEEP:
        return word
    longest = max(BY_LENGTH)
    choices = BY_LENGTH.get(min(len(word), longest))
    new = pick(key, choices)
    while len(new) < len(word):  # Longer than any vocabulary word.
        new += pick(key + new, BY_LENGTH[min(len(word) - len(new), longest)])
    if word.isupper():
        return new.upper()
    return new.capitalize() if word[0].isupper() else new


def scrub_paragraph(part, index, xml):
    pieces = [html.unescape(m.group(2)) for m in TEXT.finditer(xml)]
    text = ''.join(pieces)
    new = list(text)
    for k, m in enumerate(WORD.finditer(text)):
        new[m.start():m.end()] = replace_word(m.group(), f'{part}:{index}:{k}')
    new = ''.join(new)
    assert len(new) == len(text)
    spans = iter(pieces)
    offset = 0

    def run(m):
        nonlocal offset
        length = len(next(spans))
        chunk = new[offset:offset + length]
        offset += length
        return m.group(1) + html.escape(chunk, quote=False) + m.group(3)

    return TEXT.sub(run, xml)


def scrub_text(part, xml):
    counter = iter(range(1 << 30))
    return PARAGRAPH.sub(lambda m: scrub_paragraph(part, next(counter), m.group()), xml)


def blank_png(width, height):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    rows = b''.join(b'\0' + b'\xd8' * (width * 3) for _ in range(height))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(rows, 9)) + chunk(b'IEND', b''))


def image_size(data):
    if data.startswith(b'\x89PNG'):
        return struct.unpack('>II', data[16:24])
    i = 2  # JPEG: walk segments to the first start-of-frame marker.
    while i < len(data):
        marker, length = data[i + 1], struct.unpack('>H', data[i + 2:i + 4])[0]
        if 0xC0 <= marker <= 0xCF and marker not in (0xC4, 0xC8, 0xCC):
            height, width = struct.unpack('>HH', data[i + 5:i + 9])
            return width, height
        i += 2 + length
    raise ValueError('unsupported image')


def scrub(source, target):
    with ZipFile(source) as archive:
        parts = {info.filename: archive.read(info) for info in archive.infolist()}
    renamed = {}
    for name in list(parts):
        if name.startswith('word/media/'):
            data = blank_png(*image_size(parts.pop(name)))
            new = str(Path(name).with_suffix('.png'))
            parts[new] = data
            renamed[Path(name).name] = Path(new).name
    for name, data in list(parts.items()):
        if not name.endswith(('.xml', '.rels')):
            continue
        xml = data.decode('utf-8')
        if re.fullmatch(r'word/(document|header\d*|footer\d*|footnotes|endnotes)\.xml', name):
            xml = scrub_text(name, xml)
            xml = re.sub(r' descr="[^"]*"', ' descr="Placeholder image"', xml)
            xml = re.sub(r'(<pic:cNvPr [^>]*name=")[^"]*"', r'\1Placeholder.png"', xml)
        if name.endswith('.rels'):
            for old, new in renamed.items():
                xml = xml.replace(f'media/{old}"', f'media/{new}"')
            xml = re.sub(r'Target="[^"]*"( TargetMode="External")', r'Target="https://example.com/"\1', xml)
        if name == 'word/settings.xml':
            xml = re.sub(r'<w:docVars>.*?</w:docVars>', '', xml, flags=re.S)
        if name == 'docProps/core.xml':
            xml = re.sub(r'(<dc:creator>|<cp:lastModifiedBy>)[^<]*', r'\1mdoc fixture', xml)
            xml = re.sub(r'(<dcterms:(?:created|modified)[^>]*>)[^<]*', r'\g<1>2026-01-01T00:00:00Z', xml)
            xml = re.sub(r'<cp:lastPrinted>[^<]*</cp:lastPrinted>', '', xml)
        if name == 'docProps/app.xml':
            xml = re.sub(r'<Company>[^<]*</Company>', '<Company></Company>', xml)
        if name == 'docProps/custom.xml':
            xml = re.sub(r'<property [^>]*name="Grammarly[^"]*".*?</property>', '', xml, flags=re.S)
        parts[name] = xml.encode('utf-8')
    with ZipFile(target, 'w', ZIP_DEFLATED) as archive:
        for name in sorted(parts):
            info = ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_DEFLATED
            archive.writestr(info, parts[name])


if __name__ == '__main__':
    default = Path(__file__).with_name('legal-opinion.docx')
    scrub(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else default)
