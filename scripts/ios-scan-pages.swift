// Usage: xcrun swiftc -o <binary> scripts/ios-scan-pages.swift && <binary> <folder> <pages>
// Renders camera-scan pages of a fictional tenancy as JPEGs, the fixture
// the device UI tests' scan imports (scripts/ios-device-test.sh). The text
// matches MdocUITest.scanPages, which the Simulator tests render in-process.
import CoreGraphics
import CoreText
import Foundation
import ImageIO
import UniformTypeIdentifiers

let arguments = CommandLine.arguments
guard arguments.count == 3, let count = Int(arguments[2]), count > 0 else {
    FileHandle.standardError.write(Data("usage: ios-scan-pages <folder> <pages>\n".utf8))
    exit(2)
}
let folder = URL(fileURLWithPath: arguments[1], isDirectory: true)
try? FileManager.default.removeItem(at: folder)
try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)

let width = 1240, height = 1754
let font = CTFontCreateUIFontForLanguage(.system, 44, nil) ?? CTFontCreateWithName("Helvetica" as CFString, 44, nil)
for number in 1...count {
    guard let context = CGContext(
        data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
        space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)
    else { exit(1) }
    context.setFillColor(CGColor(gray: 1, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    let text = """
        Tenancy agreement, page \(number)

        Landlord: Harbour Street Lettings
        Tenant: Grace Whitlow
        The rent is 1,150 pounds a month, paid on day \(number).
        """
    let attributed = NSAttributedString(string: text, attributes: [
        NSAttributedString.Key(kCTFontAttributeName as String): font,
        NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(gray: 0, alpha: 1),
    ])
    // Core Graphics draws from the bottom left: the box starts 160 px from the top.
    let box = CGPath(rect: CGRect(x: 120, y: height - 160 - 1400, width: 1000, height: 1400), transform: nil)
    let framesetter = CTFramesetterCreateWithAttributedString(attributed)
    CTFrameDraw(CTFramesetterCreateFrame(framesetter, CFRange(location: 0, length: 0), box, nil), context)
    guard let image = context.makeImage() else { exit(1) }
    let url = folder.appendingPathComponent("page-\(number).jpg")
    guard let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.jpeg.identifier as CFString, 1, nil)
    else { exit(1) }
    CGImageDestinationAddImage(destination, image, [kCGImageDestinationLossyCompressionQuality: 0.9] as CFDictionary)
    guard CGImageDestinationFinalize(destination) else { exit(1) }
}
print("\(count) pages in \(folder.path)")
