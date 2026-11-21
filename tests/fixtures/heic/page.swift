// Draws a synthetic contract page (fictional text) and writes the HEIC
// variants with ImageIO. Run through make-heic.sh; macOS only.
import Foundation
import CoreGraphics
import CoreText
import ImageIO
import UniformTypeIdentifiers

let out = CommandLine.arguments[1]
let w = 1200, h = 1600

func page(width: Int, height: Int, rotated: Bool) -> CGImage {
    let cs = CGColorSpace(name: CGColorSpace.sRGB)!
    let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
                        bytesPerRow: 0, space: cs,
                        bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)!
    if rotated {
        // Store the page turned 90° counter-clockwise: EXIF orientation 6
        // ("rotate 90° clockwise to display") puts it upright again.
        ctx.translateBy(x: CGFloat(width), y: 0)
        ctx.rotate(by: .pi / 2)
    }
    ctx.setFillColor(CGColor(gray: 1, alpha: 1))
    ctx.fill(CGRect(x: 0, y: 0, width: w, height: h))
    let lines = [
        ("SERVICE AGREEMENT No. 47-B", 44.0),
        ("", 20.0),
        ("Between Northwind Ferry Company (the Client)", 30.0),
        ("and Juniper Lane Consulting LLC (the Contractor).", 30.0),
        ("", 20.0),
        ("1. The Contractor shall deliver the harbour survey", 30.0),
        ("   by 14 March 2031.", 30.0),
        ("2. The Client shall pay 12,400 credits within", 30.0),
        ("   thirty days of delivery.", 30.0),
        ("3. Notices go to 18 Gull Street, Port Avalon.", 30.0),
        ("", 20.0),
        ("All names and places in this page are fictional.", 26.0),
        ("Test fixture for mdoc, not a real document.", 26.0),
    ]
    var y = CGFloat(h) - 140
    for (text, size) in lines {
        y -= CGFloat(size) * 1.6
        if text.isEmpty { continue }
        let font = CTFontCreateWithName("Helvetica" as CFString, CGFloat(size), nil)
        let attr = NSAttributedString(string: text, attributes: [
            NSAttributedString.Key(kCTFontAttributeName as String): font,
            NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(gray: 0, alpha: 1),
        ])
        let line = CTLineCreateWithAttributedString(attr)
        ctx.textPosition = CGPoint(x: 110, y: y)
        CTLineDraw(line, ctx)
    }
    // An arrow in the top-left corner shows which way is up.
    ctx.setFillColor(CGColor(red: 0.8, green: 0.1, blue: 0.1, alpha: 1))
    ctx.move(to: CGPoint(x: 60, y: CGFloat(h) - 40))
    ctx.addLine(to: CGPoint(x: 30, y: CGFloat(h) - 90))
    ctx.addLine(to: CGPoint(x: 90, y: CGFloat(h) - 90))
    ctx.fillPath()
    return ctx.makeImage()!
}

func write(_ image: CGImage, _ name: String, _ props: [CFString: Any]) {
    let url = URL(fileURLWithPath: out).appendingPathComponent(name)
    let type = name.hasSuffix(".png") ? UTType.png : UTType.heic
    let dest = CGImageDestinationCreateWithURL(url as CFURL, type.identifier as CFString, 1, nil)!
    var p = props
    if type == .heic { p[kCGImageDestinationLossyCompressionQuality] = 0.6 }
    CGImageDestinationAddImage(dest, image, p as CFDictionary)
    precondition(CGImageDestinationFinalize(dest), "could not write \(name)")
}

let upright = page(width: w, height: h, rotated: false)
write(upright, "page-source.png", [:])
write(page(width: h, height: w, rotated: true), "page-orientation-6.heic",
      [kCGImagePropertyOrientation: 6])
write(upright, "page-gps.heic", [
    kCGImagePropertyOrientation: 1,
    kCGImagePropertyGPSDictionary: [
        kCGImagePropertyGPSLatitude: 12.3456, kCGImagePropertyGPSLatitudeRef: "N",
        kCGImagePropertyGPSLongitude: 65.4321, kCGImagePropertyGPSLongitudeRef: "W",
    ],
    kCGImagePropertyTIFFDictionary: [
        kCGImagePropertyTIFFMake: "Fixture Camera Co", kCGImagePropertyTIFFModel: "Model F1",
    ],
    kCGImagePropertyExifDictionary: [
        kCGImagePropertyExifDateTimeOriginal: "2031:03:14 09:26:53",
    ],
])
