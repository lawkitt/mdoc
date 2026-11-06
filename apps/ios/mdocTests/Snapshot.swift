import SwiftUI
import XCTest

/// Image snapshots of components and key screen states (ADR 0099), without
/// a dependency: `ImageRenderer` draws the view at a fixed width and the
/// result is compared with `Snapshots/<name>.png` beside this file.
///
/// A missing reference is recorded and the test fails once, so a new
/// snapshot is a deliberate commit. `TEST_RUNNER_MDOC_RECORD_SNAPSHOTS=1` (xcodebuild passes it on as `MDOC_RECORD_SNAPSHOTS`)
/// re-records every reference after an intended change. References come
/// from the pinned Xcode and Simulator runtime (scripts/tools.env), as the
/// iOS job uses.
enum Snapshot {
    enum Appearance: String, CaseIterable {
        case light, dark
        var scheme: ColorScheme { self == .light ? .light : .dark }
    }

    static var recording: Bool {
        ProcessInfo.processInfo.environment["MDOC_RECORD_SNAPSHOTS"] == "1"
    }

    static let folder = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("Snapshots")

    @MainActor
    static func assert(
        _ view: some View,
        named name: String,
        width: CGFloat = 390,
        appearance: Appearance = .light,
        size: DynamicTypeSize = .large,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        let content = view
            .frame(width: width)
            .background(Color(uiColor: .systemBackground))
            .environment(\.colorScheme, appearance.scheme)
            .environment(\.dynamicTypeSize, size)
        let renderer = ImageRenderer(content: content)
        renderer.scale = 2
        guard let image = renderer.uiImage, let png = image.pngData() else {
            XCTFail("\(name): nothing rendered", file: file, line: line)
            return
        }
        let reference = folder.appendingPathComponent("\(name)-\(appearance.rawValue).png")
        guard !recording, let expected = UIImage(contentsOfFile: reference.path) else {
            try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try? png.write(to: reference)
            XCTFail("\(name): recorded \(reference.lastPathComponent); run again to compare", file: file, line: line)
            return
        }
        if let difference = difference(image, expected) {
            let actual = FileManager.default.temporaryDirectory.appendingPathComponent("\(name)-\(appearance.rawValue)-actual.png")
            try? png.write(to: actual)
            add(png, named: "\(name)-actual")
            XCTFail("\(name): \(difference) (actual image attached and at \(actual.path))", file: file, line: line)
        }
    }

    /// nil when the images match: the same size, and at most 0.5% of the
    /// pixels differ by more than a small amount (anti-aliasing noise).
    static func difference(_ a: UIImage, _ b: UIImage) -> String? {
        guard let pa = pixels(a), let pb = pixels(b) else { return "unreadable image" }
        guard pa.width == pb.width, pa.height == pb.height else {
            return "size \(pa.width)×\(pa.height), expected \(pb.width)×\(pb.height)"
        }
        var differing = 0
        for i in stride(from: 0, to: pa.bytes.count, by: 4) {
            let delta = (0..<4).map { abs(Int(pa.bytes[i + $0]) - Int(pb.bytes[i + $0])) }.max() ?? 0
            if delta > 24 { differing += 1 }
        }
        let share = Double(differing) / Double(pa.width * pa.height)
        return share > 0.005 ? String(format: "%.2f%% of pixels differ", share * 100) : nil
    }

    private static func pixels(_ image: UIImage) -> (width: Int, height: Int, bytes: [UInt8])? {
        guard let cg = image.cgImage else { return nil }
        let width = cg.width, height = cg.height
        var bytes = [UInt8](repeating: 0, count: width * height * 4)
        guard let context = CGContext(
            data: &bytes, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        context.draw(cg, in: CGRect(x: 0, y: 0, width: width, height: height))
        return (width, height, bytes)
    }

    private static func add(_ png: Data, named name: String) {
        let attachment = XCTAttachment(data: png, uniformTypeIdentifier: "public.png")
        attachment.name = name
        attachment.lifetime = .keepAlways
        XCTContext.runActivity(named: name) { $0.add(attachment) }
    }
}
