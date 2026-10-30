import Foundation

/// The one place that picks the core. Until `mdoc-ffi` is wired in, the app
/// and the extension run on `FakeCore`.
enum CoreFactory {
    /// Launch argument for UI tests: instant jobs and installed models.
    static let uiTestingArgument = "-mdoc-ui-testing"

    static func app(storage: MdocStorage, arguments: [String] = ProcessInfo.processInfo.arguments) -> Core {
        var options = FakeCore.Options()
        options.originalsFolder = storage.url(.documents).appendingPathComponent("fixtures", isDirectory: true)
        if arguments.contains(uiTestingArgument) {
            options.timing = .instant
            options.modelsInstalled = true
        }
        return FakeCore.make(options)
    }

    /// The extension gets the library only: restoring needs no model.
    static func restoreExtension(storage: MdocStorage) -> any Library {
        FakeCore.make(FakeCore.Options(timing: .instant)).library
    }
}
