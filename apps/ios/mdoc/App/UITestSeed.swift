import Foundation

#if DEBUG

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
                    PendingImport(source: source, suggestedTitle: title), title: title, into: nil),
                  let model = try? app.documents.open(session.id) else { return }
            await model.recognize()
            // A scan waits here for its OCR model.
            if model.snapshot.phase == .unscanned {
                await model.findNames()
            }
            app.library.refresh()
        }
    }

    /// The Restore extension's device test (`MDOC_SEED_RESTORE=1`): a
    /// Project with a document and an Unfiled document, each with an email
    /// replaced, so both can be shared at once.
    static let restoreProject = "Restore check"
    static let restoreProjectDocument = "Project letter"
    static let restoreUnfiledDocument = "Unfiled note"

    static func seedRestorePlaces(in app: AppModel) {
        Task {
            guard let library = app.core.library as? RealLibrary,
                  let project = try? library.vault.createProject(name: restoreProject) else { return }
            let seeds: [(String, String, ProjectID?)] = [
                (restoreProjectDocument, "Write to maya.ellison@example.com today.", ProjectID(rawValue: project.id)),
                (restoreUnfiledDocument, "Ask ann.baker@example.com first.", nil),
            ]
            for (title, text, project) in seeds {
                guard let session = try? app.library.importDocument(
                        PendingImport(source: .text(text), suggestedTitle: title), title: title, into: project),
                      let model = try? app.documents.open(session.id) else { return }
                await model.recognize()
                await model.findNames()
                model.replaceAllPending()
            }
            app.saveChanges()
            app.library.refresh()
        }
    }

    private static func source(_ file: URL, storage: MdocStorage) -> ImportSource? {
        var isFolder: ObjCBool = false
        if FileManager.default.fileExists(atPath: file.path, isDirectory: &isFolder), isFolder.boolValue {
            let folder = storage.url(.intake).appendingPathComponent(UUID().uuidString, isDirectory: true)
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
        guard let copy = try? storage.importCopy(of: file, in: .intake),
              let kind = Importer.kind(of: copy) else { return nil }
        return .file(copy, kind)
    }
}
#endif
