import Foundation

/// UI tests' seed (`LaunchConfiguration.seedFile`): a fixture imported
/// through the core at launch and scanned with the structured rules, so the
/// review has candidates without a model. This is the only scan below the
/// detector floor: the app itself offers no Find names there (ADR 0088). A
/// Markdown file comes in as pasted text, and a folder of JPEG pages as a
/// camera scan.
@MainActor
enum UITestSeed {
    /// Where `just ios-device-test` copies the UI tests' fixtures on a phone:
    /// this folder in the app's data container (the app ships none).
    static let deviceFixturesFolder = "Library/mdoc-ui-fixtures"

    /// A UI test's fixture: its path on the Mac, which the Simulator reads,
    /// or on a phone, the copy of the same name in `deviceFixturesFolder`.
    static func fixture(_ path: String) -> URL {
        let mac = URL(fileURLWithPath: path)
        if FileManager.default.fileExists(atPath: mac.path) {
            return mac
        }
        return URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
            .appendingPathComponent(deviceFixturesFolder, isDirectory: true)
            .appendingPathComponent(mac.lastPathComponent)
    }

    static func run(_ file: URL, in app: AppModel) {
        Task {
            let title = file.deletingPathExtension().lastPathComponent
            guard let source = source(file, storage: app.storage),
                  let session = try? app.library.importDocument(
                    PendingImport(source: source, suggestedTitle: title), title: title, into: .newCase(name: title)),
                  let model = try? app.documents.open(session.id) else { return }
            await model.recognize()
            // A scan waits here for its OCR model.
            if model.snapshot.phase == .unscanned {
                await model.findNames()
            }
            app.library.refresh()
        }
    }

    private static func source(_ file: URL, storage: MdocStorage) -> ImportSource? {
        var isFolder: ObjCBool = false
        if FileManager.default.fileExists(atPath: file.path, isDirectory: &isFolder), isFolder.boolValue {
            let folder = storage.url(.originals).appendingPathComponent(UUID().uuidString, isDirectory: true)
            let pages = ((try? FileManager.default.contentsOfDirectory(at: file, includingPropertiesForKeys: nil)) ?? [])
                .filter { $0.pathExtension.lowercased() == "jpg" }
                .sorted { $0.lastPathComponent < $1.lastPathComponent }
            guard (try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)) != nil,
                  let copies = try? pages.map({ page in
                      let copy = folder.appendingPathComponent(page.lastPathComponent)
                      try FileManager.default.copyItem(at: page, to: copy)
                      return copy
                  }) else { return nil }
            return .scan(copies)
        }
        if file.pathExtension.lowercased() == "md" {
            return (try? String(contentsOf: file, encoding: .utf8)).map { .text($0) }
        }
        guard let copy = try? storage.importCopy(of: file, in: .originals),
              let kind = Importer.kind(of: copy) else { return nil }
        return .file(copy, kind)
    }
}
