// Usage: swift tools/ocr-upstream/small_print.swift OUTPUT [SOURCE_DPI]
// Small-print probe pages for `dpi.py`: a fictional 30-row statement in
// English and Russian at 6, 7, 8, 9 and 10 pt, each an image-only A4 PDF
// rasterized at SOURCE_DPI (300 by default, a flatbed scan), with the
// adjacent `.txt` reference. Self-made text (CC0), Arial, black on white.
import AppKit
import CoreText

let arguments = CommandLine.arguments
guard arguments.count >= 2 else {
    FileHandle.standardError.write("usage: small_print.swift OUTPUT [SOURCE_DPI]\n".data(using: .utf8)!)
    exit(2)
}
let output = URL(fileURLWithPath: arguments[1], isDirectory: true)
let sourceDPI = arguments.count > 2 ? Double(arguments[2])! : 300
try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)

let payees = [
    "en": ["Harbour Street Lettings", "Northgate Water Ltd", "Copperfield Insurance", "Alder & Finch LLP",
           "City Council Tax", "Brightline Energy"],
    "ru": ["ООО «Северный двор»", "АО «Городской водоканал»", "СК «Надёжный щит»", "Адвокатское бюро «Ольха»",
           "Управа района Хамовники", "ПАО «Энергосбыт»"],
]
let words = [
    "en": (title: "Account statement", invoice: "Invoice No.", items: ["rent", "water", "insurance", "legal fees", "council tax", "electricity"]),
    "ru": (title: "Выписка по счёту", invoice: "Счёт №", items: ["аренда", "водоснабжение", "страховка", "юридические услуги", "налог", "электроэнергия"]),
]

/// Thirty rows: date, invoice, payee, item and amount, all derived from the row number.
func lines(_ language: String, size: Int) -> [String] {
    let w = words[language]!
    var rows = ["\(w.title), \(size) pt"]
    for row in 0..<30 {
        let day = String(format: "%02d.%02d.2026", row % 28 + 1, row % 12 + 1)
        let cents = (row * 7919 + 1234) % 500_000 + 1000
        let units = cents / 100
        let thousands = units / 1000, rest = units % 1000
        let amount: String
        if language == "en" {
            amount = (thousands > 0 ? "\(thousands),\(String(format: "%03d", rest))" : "\(rest)")
                + String(format: ".%02d", cents % 100)
        } else {
            amount = (thousands > 0 ? "\(thousands) \(String(format: "%03d", rest))" : "\(rest)")
                + String(format: ",%02d", cents % 100)
        }
        let payee = payees[language]![row % 6], item = w.items[(row * 5) % 6]
        rows.append("\(day)  \(w.invoice) \(10400 + row * 37)  \(payee)  \(item)  \(amount)")
    }
    return rows
}

func render(_ language: String, size: Int) throws {
    let scale = sourceDPI / 72
    let width = Int(595 * scale), height = Int(842 * scale)
    let gray = CGColorSpaceCreateDeviceGray()
    let bitmap = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                           space: gray, bitmapInfo: CGImageAlphaInfo.none.rawValue)!
    bitmap.setFillColor(gray: 1, alpha: 1)
    bitmap.fill(CGRect(x: 0, y: 0, width: width, height: height))
    let font = CTFontCreateWithName("ArialMT" as CFString, CGFloat(Double(size) * scale), nil)
    let attributes: [NSAttributedString.Key: Any] = [
        .font: font, .foregroundColor: CGColor(gray: 0, alpha: 1),
    ]
    let text = lines(language, size: size)
    var y = Double(height) - 72 * scale
    for line in text {
        y -= Double(size) * 1.4 * scale
        let ctLine = CTLineCreateWithAttributedString(NSAttributedString(string: line, attributes: attributes))
        bitmap.textPosition = CGPoint(x: 72 * scale, y: y)
        CTLineDraw(ctLine, bitmap)
    }
    let image = bitmap.makeImage()!
    let name = "\(language)-\(size)pt"
    var box = CGRect(x: 0, y: 0, width: 595, height: 842)
    let pdf = CGContext(output.appendingPathComponent("\(name).pdf") as CFURL, mediaBox: &box, nil)!
    pdf.beginPDFPage(nil)
    pdf.draw(image, in: box)
    pdf.endPDFPage()
    pdf.closePDF()
    try (text.joined(separator: "\n") + "\n").write(
        to: output.appendingPathComponent("\(name).txt"), atomically: true, encoding: .utf8)
}

for language in ["en", "ru"] {
    for size in [6, 7, 8, 9, 10] {
        try render(language, size: size)
    }
}
