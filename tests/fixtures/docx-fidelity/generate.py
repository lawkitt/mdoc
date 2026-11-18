"""Generate the DOCX fidelity fixtures: eight synthetic EN/RU legal documents.

Standard library only, deterministic, no private text: every sentence is
written for these fixtures. Run from the repository root:

    python3 tests/fixtures/docx-fidelity/generate.py
"""
from pathlib import Path
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED
from xml.sax.saxutils import escape

W = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main'
R = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
PKG = 'http://schemas.openxmlformats.org/package/2006/relationships'
CT = 'application/vnd.openxmlformats-officedocument.wordprocessingml'
HERE = Path(__file__).parent
ANNOTATION_REF = '<w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:annotationRef/></w:r>'


def run(text, rpr=''):
    props = f'<w:rPr>{rpr}</w:rPr>' if rpr else ''
    return f'<w:r>{props}<w:t xml:space="preserve">{escape(text)}</w:t></w:r>'


def para(content, ppr=''):
    props = f'<w:pPr>{ppr}</w:pPr>' if ppr else ''
    if isinstance(content, str):
        content = run(content)
    return f'<w:p>{props}{content}</w:p>'


def heading(text, level=1):
    return para(text, f'<w:pStyle w:val="Heading{level}"/>')


def item(text, level=0, num=1):
    return para(text, f'<w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="{num}"/></w:numPr>')


def field(instr, cached):
    return (f'<w:r><w:fldChar w:fldCharType="begin"/></w:r>'
            f'<w:r><w:instrText xml:space="preserve"> {instr} </w:instrText></w:r>'
            f'<w:r><w:fldChar w:fldCharType="separate"/></w:r>{run(cached)}'
            f'<w:r><w:fldChar w:fldCharType="end"/></w:r>')


def table(rows, widths, header=True, merges=None):
    """rows: lists of cell texts; merges: {(row, col): 'span' | 'restart' | 'continue'}."""
    merges = merges or {}
    border = ''.join(f'<w:{side} w:val="single" w:sz="4" w:space="0" w:color="808080"/>'
                     for side in ('top', 'left', 'bottom', 'right', 'insideH', 'insideV'))
    out = (f'<w:tbl><w:tblPr><w:tblW w:w="{sum(widths)}" w:type="dxa"/><w:tblBorders>{border}</w:tblBorders>'
           f'<w:tblLayout w:type="fixed"/></w:tblPr><w:tblGrid>'
           + ''.join(f'<w:gridCol w:w="{w}"/>' for w in widths) + '</w:tblGrid>')
    for r, cells in enumerate(rows):
        out += '<w:tr>'
        c = 0
        for text in cells:
            merge = merges.get((r, c))
            span = 1
            tcpr = ''
            if isinstance(merge, int):
                span = merge
                tcpr += f'<w:gridSpan w:val="{span}"/>'
            elif merge == 'restart':
                tcpr += '<w:vMerge w:val="restart"/>'
            elif merge == 'continue':
                tcpr += '<w:vMerge/>'
            width = sum(widths[c:c + span])
            shade = '<w:shd w:val="clear" w:color="auto" w:fill="E7E6E6"/>' if header and r == 0 else ''
            rpr = '<w:b/>' if header and r == 0 else ''
            out += (f'<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/>{tcpr}{shade}</w:tcPr>'
                    f'{para(run(text, rpr))}</w:tc>')
            c += span
        out += '</w:tr>'
    return out + '</w:tbl>'


def styles(font, size=22, line=276, after=160, justify=False):
    jc = '<w:jc w:val="both"/>' if justify else ''
    return (f'<w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr>'
            f'<w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}" w:eastAsia="{font}"/>'
            f'<w:sz w:val="{size}"/><w:szCs w:val="{size}"/><w:lang w:val="en-GB" w:eastAsia="en-US" w:bidi="ar-SA"/>'
            f'</w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="{after}" w:line="{line}" w:lineRule="auto"/>{jc}</w:pPr></w:pPrDefault></w:docDefaults>'
            '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>'
            '<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/>'
            '<w:pPr><w:keepNext/><w:spacing w:before="240" w:after="120"/><w:jc w:val="left"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="28"/><w:szCs w:val="28"/></w:rPr></w:style>'
            '<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/>'
            '<w:pPr><w:keepNext/><w:spacing w:before="160" w:after="80"/><w:jc w:val="left"/><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:sz w:val="24"/><w:szCs w:val="24"/></w:rPr></w:style>'
            '<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:qFormat/>'
            '<w:pPr><w:spacing w:after="240"/><w:jc w:val="center"/></w:pPr><w:rPr><w:b/><w:caps/><w:sz w:val="32"/><w:szCs w:val="32"/></w:rPr></w:style>'
            '<w:style w:type="paragraph" w:styleId="Header"><w:name w:val="header"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:rPr><w:sz w:val="18"/></w:rPr></w:style>'
            '<w:style w:type="paragraph" w:styleId="Footer"><w:name w:val="footer"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/><w:jc w:val="center"/></w:pPr><w:rPr><w:sz w:val="18"/></w:rPr></w:style>'
            '<w:style w:type="paragraph" w:styleId="FootnoteText"><w:name w:val="footnote text"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:rPr><w:sz w:val="18"/></w:rPr></w:style>'
            '<w:style w:type="character" w:styleId="FootnoteReference"><w:name w:val="footnote reference"/><w:rPr><w:vertAlign w:val="superscript"/></w:rPr></w:style>'
            '<w:style w:type="character" w:styleId="CommentReference"><w:name w:val="annotation reference"/><w:rPr><w:sz w:val="16"/></w:rPr></w:style>'
            '<w:style w:type="character" w:styleId="Hyperlink"><w:name w:val="Hyperlink"/><w:rPr><w:color w:val="0563C1"/><w:u w:val="single"/></w:rPr></w:style>'
            '</w:styles>')


def numbering(formats):
    """formats: per numId, a list of (numFmt, lvlText) per level."""
    out = f'<w:numbering xmlns:w="{W}">'
    for n, levels in enumerate(formats, start=1):
        out += f'<w:abstractNum w:abstractNumId="{n}"><w:multiLevelType w:val="multilevel"/>'
        for i, (fmt, text) in enumerate(levels):
            indent = 425 + 425 * i
            out += (f'<w:lvl w:ilvl="{i}"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/><w:lvlText w:val="{text}"/>'
                    f'<w:lvlJc w:val="left"/><w:pPr><w:ind w:left="{indent}" w:hanging="425"/></w:pPr></w:lvl>')
        out += '</w:abstractNum>'
    for n in range(1, len(formats) + 1):
        out += f'<w:num w:numId="{n}"><w:abstractNumId w:val="{n}"/></w:num>'
    return out + '</w:numbering>'


def sect(header=False, footer=False, a4=True):
    refs = ''
    if header:
        refs += f'<w:headerReference w:type="default" r:id="header"/>'
    if footer:
        refs += f'<w:footerReference w:type="default" r:id="footer"/>'
    size = '<w:pgSz w:w="11906" w:h="16838"/>' if a4 else '<w:pgSz w:w="12240" w:h="15840"/>'
    return (f'<w:sectPr>{refs}{size}<w:pgMar w:top="1418" w:right="1134" w:bottom="1418" w:left="1701" '
            f'w:header="709" w:footer="709" w:gutter="0"/></w:sectPr>')


def build(name, body, *, font, header=None, footer=None, comments=None, footnotes=None,
          numbers=None, links=None, a4=True, **style):
    parts = {}
    rels = [('styles', 'styles', 'styles.xml', '')]
    types = {'document': 'document.main', 'styles': 'styles'}
    parts['word/styles.xml'] = styles(font, **style)
    if numbers:
        parts['word/numbering.xml'] = numbering(numbers)
        rels.append(('numbering', 'numbering', 'numbering.xml', ''))
        types['numbering'] = 'numbering'
    if header:
        parts['word/header1.xml'] = f'<w:hdr xmlns:w="{W}" xmlns:r="{R}">{header}</w:hdr>'
        rels.append(('header', 'header', 'header1.xml', ''))
        types['header1'] = 'header'
    if footer:
        parts['word/footer1.xml'] = f'<w:ftr xmlns:w="{W}" xmlns:r="{R}">{footer}</w:ftr>'
        rels.append(('footer', 'footer', 'footer1.xml', ''))
        types['footer1'] = 'footer'
    if comments:
        parts['word/comments.xml'] = f'<w:comments xmlns:w="{W}">' + ''.join(
            f'<w:comment w:id="{cid}" w:author="{escape(author)}" w:date="2026-11-18T09:00:00Z" w:initials="{escape(author[:2])}">'
            f'{para(ANNOTATION_REF + run(text))}</w:comment>'
            for cid, author, text in comments) + '</w:comments>'
        rels.append(('comments', 'comments', 'comments.xml', ''))
        types['comments'] = 'comments'
    if footnotes:
        separators = ('<w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>'
                      '<w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>')
        notes = ''.join(
            f'<w:footnote w:id="{fid}"><w:p><w:pPr><w:pStyle w:val="FootnoteText"/></w:pPr>'
            f'<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteRef/></w:r>{run(" " + text)}</w:p></w:footnote>'
            for fid, text in footnotes)
        parts['word/footnotes.xml'] = f'<w:footnotes xmlns:w="{W}">{separators}{notes}</w:footnotes>'
        rels.append(('footnotes', 'footnotes', 'footnotes.xml', ''))
        types['footnotes'] = 'footnotes'
    for rid, target in (links or []):
        rels.append((rid, 'hyperlink', target, ' TargetMode="External"'))
    parts['word/document.xml'] = (f'<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>{body}'
                                  f'{sect(bool(header), bool(footer), a4)}</w:body></w:document>')
    parts['word/_rels/document.xml.rels'] = f'<Relationships xmlns="{PKG}">' + ''.join(
        f'<Relationship Id="{rid}" Type="{R}/{kind}" Target="{escape(target)}"{mode}/>'
        for rid, kind, target, mode in rels) + '</Relationships>'
    parts['_rels/.rels'] = (f'<Relationships xmlns="{PKG}"><Relationship Id="document" '
                            f'Type="{R}/officeDocument" Target="word/document.xml"/></Relationships>')
    parts['[Content_Types].xml'] = (
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        + ''.join(f'<Override PartName="/word/{part}.xml" ContentType="{CT}.{kind}+xml"/>'
                  for part, kind in types.items()) + '</Types>')
    with ZipFile(HERE / name, 'w') as archive:
        for part, data in sorted(parts.items()):
            info = ZipInfo(part, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_DEFLATED
            archive.writestr(info, data.encode())


def page_footer(word):
    return para(run(f'{word} ') + field('PAGE', '1') + run(' / ') + field('NUMPAGES', '1'), '<w:pStyle w:val="Footer"/>')


def header_line(text):
    return para(text, '<w:pStyle w:val="Header"/><w:jc w:val="right"/>'
                '<w:pBdr><w:bottom w:val="single" w:sz="4" w:space="1" w:color="808080"/></w:pBdr>')


# 1. English services agreement: multi-level numbering, header and page footer.
EN_CLAUSES = [
    ('Services', ['The Supplier shall provide the services described in Schedule 1 with reasonable skill and care, in accordance with good industry practice.',
                  'The Supplier shall assign suitably qualified personnel and shall replace any member of personnel whom the Customer reasonably considers unsuitable.',
                  'Any change to the scope of the services shall be agreed in writing under clause 9.']),
    ('Fees and payment', ['The Customer shall pay the fees set out in Schedule 2 within thirty days of receipt of a valid invoice.',
                          'Interest on late payment accrues at four per cent above the base rate from time to time, calculated daily.',
                          'All sums are exclusive of value added tax, which the Customer shall pay in addition at the applicable rate.']),
    ('Confidentiality', ['Each party shall keep the other party’s confidential information secret and shall use it only to perform this agreement.',
                         'This clause does not apply to information that is or becomes public other than through a breach of this agreement.']),
    ('Liability', ['Nothing in this agreement limits liability for death or personal injury caused by negligence, or for fraud.',
                   'Subject to clause 4.1, each party’s total liability in each contract year shall not exceed the fees paid in that year.',
                   'Neither party is liable for loss of profits, loss of business or any indirect or consequential loss.']),
    ('Term and termination', ['This agreement starts on the Commencement Date and continues for an initial term of two years.',
                              'Either party may terminate this agreement with immediate effect by written notice if the other party commits a material breach which is not remedied within twenty days.']),
    ('General', ['This agreement is the entire agreement between the parties.', 'No variation is effective unless it is in writing and signed by both parties.',
                 'This agreement is governed by the law of England and Wales, and the courts of England have exclusive jurisdiction.']),
]
body = para('Services agreement', '<w:pStyle w:val="Title"/>')
body += para(run('This agreement is dated ') + run('18 November 2026', '<w:b/>') + run(' and made between the parties below.'))
body += item('Northgate Analytics Limited, a company registered in England under number 08000001 (the Supplier); and', 0, 2)
body += item('Riverside Holdings plc, a company registered in England under number 08000002 (the Customer).', 0, 2)
for title, subs in EN_CLAUSES:
    body += item(title, 0, 1).replace('<w:pPr>', '<w:pPr><w:pStyle w:val="Heading2"/>', 1)
    for text in subs:
        body += item(text, 1, 1)
body += para('Signed for and on behalf of the parties by their authorised signatories.')
build('en-services-agreement.docx', body, font='Calibri', header=header_line('Services agreement · Draft for discussion'),
      footer=page_footer('Page'), numbers=[[('decimal', '%1.'), ('decimal', '%1.%2'), ('lowerLetter', '(%3)')],
                                           [('lowerLetter', '(%1)')]])

# 2. Russian supply contract: numbering, a goods table with rouble amounts, header and footer.
body = para('Договор поставки № 18/11-26', '<w:pStyle w:val="Title"/>')
body += table([['г. Москва', '18 ноября 2026 г.']], [4535, 4536], header=False).replace('<w:tblBorders>', '<w:tblBorders><w:top w:val="nil"/><w:left w:val="nil"/><w:bottom w:val="nil"/><w:right w:val="nil"/><w:insideH w:val="nil"/><w:insideV w:val="nil"/>', 1)
body += para('Общество с ограниченной ответственностью «Северный склад», именуемое в дальнейшем «Поставщик», в лице генерального директора, действующего на основании устава, с одной стороны, и акционерное общество «Речной порт», именуемое в дальнейшем «Покупатель», с другой стороны, заключили настоящий договор о нижеследующем.')
RU_CLAUSES = [
    ('Предмет договора', ['Поставщик обязуется передать в собственность Покупателя товар, а Покупатель обязуется принять и оплатить его.',
                          'Наименование, количество и цена товара указаны в спецификации, приведённой ниже.']),
    ('Цена и порядок расчётов', ['Общая цена договора составляет 1 250 000 ₽, включая НДС.',
                                 'Покупатель оплачивает товар в течение тридцати календарных дней с даты подписания товарной накладной.',
                                 'Датой оплаты считается дата зачисления денежных средств на расчётный счёт Поставщика.']),
    ('Сроки и порядок поставки', ['Поставка осуществляется партиями по заявкам Покупателя.',
                                  'Право собственности и риск случайной гибели товара переходят к Покупателю с момента передачи товара.']),
    ('Ответственность сторон', ['За просрочку оплаты Покупатель уплачивает неустойку в размере 0,1 % от неоплаченной суммы за каждый день просрочки.',
                                'Стороны освобождаются от ответственности за неисполнение обязательств, вызванное обстоятельствами непреодолимой силы.']),
]
for title, subs in RU_CLAUSES:
    body += item(title, 0, 1).replace('<w:pPr>', '<w:pPr><w:pStyle w:val="Heading2"/>', 1)
    for text in subs:
        body += item(text, 1, 1)
body += heading('Спецификация', 2)
body += table([['№', 'Наименование товара', 'Кол-во', 'Цена, ₽', 'Сумма, ₽'],
               ['1', 'Стеллаж складской металлический', '20', '25 000,00', '500 000,00'],
               ['2', 'Тележка гидравлическая', '10', '45 000,00', '450 000,00'],
               ['3', 'Контейнер пластиковый', '200', '1 500,00', '300 000,00'],
               ['', 'Итого', '', '', '1 250 000,00']], [567, 3969, 1134, 1700, 1701],
              merges={(4, 1): 3})
body += para(run('Итого к оплате: ') + run('один миллион двести пятьдесят тысяч рублей 00 копеек', '<w:b/>') + run('.'))
build('ru-supply-contract.docx', body, font='Times New Roman', size=24, line=240, after=120, justify=True,
      header=header_line('Договор поставки № 18/11-26'), footer=page_footer('Страница'),
      numbers=[[('decimal', '%1.'), ('decimal', '%1.%2.')]])

# 3. English NDA with comments: ranges over lines and paragraphs, overlap, point comments.
def cs(i): return f'<w:commentRangeStart w:id="{i}"/>'
def ce(i): return f'<w:commentRangeEnd w:id="{i}"/><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="{i}"/></w:r>'
def ref(i): return f'<w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="{i}"/></w:r>'
body = para('Mutual non-disclosure agreement', '<w:pStyle w:val="Title"/>')
body += heading('1. Purpose')
body += para(cs(0) + run('The parties intend to discuss a possible acquisition of the Target and will exchange confidential information for that purpose only. Neither party is obliged to proceed with the transaction, and either may end the discussions at any time without giving reasons.') + ce(0))
body += heading('2. Confidential information')
body += para(run('Confidential information means ') + cs(1) + run('all information, in any form, disclosed by one party to the other ') + cs(2) + run('in connection with the purpose') + ce(1) + run(', including the existence of these discussions') + ce(2) + run('.'))
body += para(run('It does not include information that the receiving party can show was lawfully in its possession before disclosure.') + ref(3))
body += heading('3. Obligations')
body += para(cs(4) + run('The receiving party shall keep the confidential information secret and shall not disclose it to any person other than its advisers who need to know it.'))
body += para(run('The receiving party shall return or destroy the confidential information on request, except for copies it must keep to comply with law.') + ce(4))
body += heading('4. Term')
body += para(run('The obligations in this agreement last for three years from the date of this agreement.'))
build('en-nda-comments.docx', body, font='Cambria', justify=True, footer=page_footer('Page'),
      comments=[(0, 'Helen Ward', 'Confirm the purpose matches the term sheet.'), (1, 'Helen Ward', 'Too broad? Consider limiting to marked information.'),
                (2, 'Omar Reyes', 'Keep the existence of talks confidential.'), (3, 'Omar Reyes', 'Add the usual carve-out for independently developed information.'),
                (4, 'Helen Ward', 'Check against the client’s standard form.')])

# 4. Russian letter with comments: Russian authors and text, a point comment, a range over a paragraph break.
body = para(run('ООО «Северный склад»', '<w:b/>'), '<w:jc w:val="right"/><w:spacing w:after="0"/>')
body += para('генеральному директору', '<w:jc w:val="right"/><w:spacing w:after="0"/>')
body += para('А. В. Морозову', '<w:jc w:val="right"/>')
body += para(run('Претензия', '<w:b/><w:sz w:val="28"/>'), '<w:jc w:val="center"/><w:spacing w:before="240"/>')
body += para(run('Между нашими организациями заключён договор поставки № 18/11-26 от 18 ноября 2026 г. ') + cs(0) + run('Согласно пункту 2.2 договора оплата товара производится в течение тридцати календарных дней.') + ce(0))
body += para(cs(1) + run('Товар по товарной накладной № 45 был передан с нарушением срока на двенадцать дней.'))
body += para(run('В связи с этим просим уплатить неустойку в размере 15 000 ₽ в течение десяти дней с даты получения настоящей претензии.') + ce(1))
body += para(run('В случае неисполнения требования мы будем вынуждены обратиться в арбитражный суд.') + ref(2))
body += para(run('Приложение: копия товарной накладной на 1 л.'), '<w:spacing w:before="240"/>')
body += table([['Директор по правовым вопросам', '', 'Е. С. Лебедева']], [4000, 2500, 2571], header=False).replace('<w:tblBorders>', '<w:tblBorders><w:top w:val="nil"/><w:left w:val="nil"/><w:bottom w:val="nil"/><w:right w:val="nil"/><w:insideH w:val="nil"/><w:insideV w:val="nil"/>', 1)
build('ru-claim-comments.docx', body, font='Calibri', justify=True,
      comments=[(0, 'Ирина Котова', 'Проверить номер пункта по последней редакции.'), (1, 'Павел Смирнов', 'Сумму неустойки пересчитать.'),
                (2, 'Ирина Котова', 'Указать подсудность.')])

# 5. English memo with hidden text: Word's PDF export leaves it out.
VANISH = '<w:vanish/>'
body = para('Memorandum', '<w:pStyle w:val="Title"/>')
body += table([['To', 'Litigation team'], ['From', 'Knowledge management'], ['Date', '18 November 2026'], ['Re', 'Limitation periods for contract claims']], [1800, 7271], header=False)
body += heading('Summary')
body += para(run('A claim for breach of contract must generally be brought within six years of the breach.') + run(' Internal note: confirm with the partner before sending.', VANISH))
body += para(run('For a contract executed as a deed the period is twelve years.') + run(' Hidden drafting comment about deeds.', VANISH) + run(' Time runs from the date of breach, not from the date of loss.'))
body += para(run('Hidden paragraph that the export omits.', VANISH), f'<w:rPr>{VANISH}</w:rPr>')
body += heading('Exceptions')
body += item('Fraud, concealment or mistake postpone the start of the period until discovery.', 0, 1)
body += item('A written acknowledgment or part payment restarts the period for a debt.', 0, 1)
body += item('Claims for personal injury follow a separate three-year rule.' + '', 0, 1).replace('</w:p>', run(' (Not relevant here.)', VANISH) + '</w:p>')
body += para('Please contact the knowledge team with any questions.')
build('en-memo-hidden-text.docx', body, font='Arial', size=21, footer=page_footer('Page'), a4=False,
      numbers=[[('bullet', '•')]])

# 6. Russian tables: merged cells, shading and borders, a tariff schedule.
body = para('Приложение № 2 к договору оказания услуг', '<w:pStyle w:val="Title"/>')
body += heading('Тарифы на юридические услуги', 2)
body += table([['Вид услуги', 'Единица', 'Ставка, ₽', 'Примечание'],
               ['Консультации', 'час', '12 000', 'Не менее 1 часа'],
               ['Подготовка договора', 'документ', '45 000', 'До 20 страниц'],
               ['Представительство в суде', 'заседание', '60 000', 'Первая инстанция'],
               ['', 'заседание', '75 000', 'Апелляция'],
               ['Due diligence', 'проект', 'по соглашению', '']],
              [3200, 1500, 1800, 2571], merges={(3, 0): 'restart', (4, 0): 'continue'})
body += heading('Сводная таблица расходов', 2)
body += table([['Статья', 'I квартал', 'II квартал', 'Итого'],
               ['Гонорар', '360 000', '420 000', '780 000'],
               ['Государственная пошлина', '15 000', '—', '15 000'],
               ['Командировочные расходы', '28 500', '31 200', '59 700'],
               ['Всего', '403 500', '451 200', '854 700']], [3500, 1857, 1857, 1857])
body += para('Ставки указаны без учёта НДС. Расходы возмещаются по фактическим документам.')
build('ru-tariff-tables.docx', body, font='Arial', size=20, line=240, after=120)

# 7. English letter with footnotes, a letterhead header and a link.
body = para(run('Elm Row Legal LLP', '<w:b/><w:sz w:val="28"/>'), '<w:spacing w:after="0"/>')
body += para('12 Elm Row, Edinburgh EH7 4AA', '<w:spacing w:after="240"/>')
body += para('18 November 2026')
body += para('Dear Ms Shah,')
body += para(run('Re: Termination of the distribution agreement', '<w:b/>'))
body += para(run('We act for Riverside Holdings plc in connection with the distribution agreement dated 1 March 2024.') +
             '<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="1"/></w:r>' +
             run(' Our client gives notice that it terminates the agreement with effect from 31 December 2026, in accordance with clause 14.2.'))
body += para(run('The notice period of three months has been observed. Our client reserves all rights in respect of the outstanding invoices.') +
             '<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="2"/></w:r>')
body += para(run('Further information about the procedure is published at ') +
             '<w:hyperlink r:id="link"><w:r><w:rPr><w:rStyle w:val="Hyperlink"/></w:rPr><w:t>www.example.com/notices</w:t></w:r></w:hyperlink>' + run('.'))
body += para('Please acknowledge receipt of this letter by return.')
body += para('Yours sincerely,', '<w:spacing w:before="240" w:after="600"/>')
body += para('Elm Row Legal LLP')
build('en-letter-footnotes.docx', body, font='Times New Roman', size=24, justify=True,
      header=header_line('Private and confidential'), footer=page_footer('Page'),
      footnotes=[(1, 'A copy of the agreement is enclosed.'), (2, 'Invoices 2026/114 to 2026/119, totalling £18,240.')],
      links=[('link', 'https://www.example.com/notices')])

# 8. Russian power of attorney: lettered sub-items, bullets and a signature table.
body = para('Доверенность № 7', '<w:pStyle w:val="Title"/>')
body += para('город Санкт-Петербург, восемнадцатое ноября две тысячи двадцать шестого года', '<w:jc w:val="center"/>')
body += para('Акционерное общество «Речной порт» в лице генерального директора, действующего на основании устава, настоящей доверенностью уполномочивает Лебедеву Елену Сергеевну представлять интересы общества по следующим вопросам:')
for text, subs in [('представлять общество во всех судах Российской Федерации со всеми правами истца, ответчика и третьего лица, в том числе:',
                    ['подписывать исковые заявления и отзывы;', 'заключать мировые соглашения;', 'обжаловать судебные акты;']),
                   ('получать и подавать документы в государственных органах;', []),
                   ('совершать иные действия, связанные с выполнением настоящего поручения.', [])]:
    body += item(text, 0, 1)
    for sub in subs:
        body += item(sub, 1, 1)
body += para('Доверенность выдана без права передоверия сроком на один год.')
body += heading('Особые условия', 2)
for text in ['Представитель обязан ежемесячно отчитываться о ходе дел.', 'Доверенность может быть отозвана в любое время.']:
    body += item(text, 0, 2)
body += table([['Генеральный директор', '______________', 'А. Н. Волков'], ['М. П.', '', '']], [3500, 2785, 2786], header=False).replace('<w:tblBorders>', '<w:tblBorders><w:top w:val="nil"/><w:left w:val="nil"/><w:bottom w:val="nil"/><w:right w:val="nil"/><w:insideH w:val="nil"/><w:insideV w:val="nil"/>', 1)
build('ru-power-of-attorney.docx', body, font='Cambria', justify=True, footer=page_footer('Страница'),
      numbers=[[('decimal', '%1.'), ('russianLower', '%2)')], [('bullet', '–')]])
