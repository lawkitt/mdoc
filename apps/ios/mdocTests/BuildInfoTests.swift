import XCTest
@testable import mdoc

/// What an App Store upload needs from the app's bundle (ADR 0097): a build
/// number and core commit from git, the export-compliance key and the privacy
/// manifest. The unit tests run inside the app, so `Bundle.main` is mdoc.
final class BuildInfoTests: XCTestCase {
    func testAHashLinksItsSourceTree() {
        let info = BuildInfo(info: [
            "CFBundleShortVersionString": "0.1.0",
            "CFBundleVersion": "718",
            "MdocCoreCommit": "923FFE1B0C2D4E5F60718293A4B5C6D7E8F90A1B",
        ])
        XCTAssertEqual(info.versionText, "0.1.0 (718)")
        XCTAssertEqual(info.shortCommit, "923ffe1")
        XCTAssertEqual(
            info.sourceURL.absoluteString,
            "https://github.com/lawkitt/mdoc/tree/923ffe1b0c2d4e5f60718293a4b5c6d7e8f90a1b")
    }

    func testABuildWithoutGitLinksTheRepository() {
        for raw in ["unknown", "", "$(MDOC_CORE_COMMIT)"] {
            let info = BuildInfo(info: ["MdocCoreCommit": raw])
            XCTAssertNil(info.commit, raw)
            XCTAssertNil(info.shortCommit, raw)
            XCTAssertEqual(info.sourceURL, BuildInfo.repositoryURL, raw)
        }
        XCTAssertEqual(BuildInfo(info: nil).versionText, "? (?)")
    }

    func testTheAppCarriesItsBuildNumberAndCommit() throws {
        let info = BuildInfo.current
        XCTAssertNotNil(info.commit, "MdocCoreCommit is not a commit: run `just ios-core` first")
        let build = try XCTUnwrap(Int(info.build), "CFBundleVersion \(info.build) is not a number")
        XCTAssertGreaterThan(build, 0)
    }

    /// NO, as App Store Connect asks for an app whose only encryption beyond
    /// Apple's is a standard algorithm (the core's AES-GCM) and which is not
    /// on the App Store in France: no documentation is needed. YES made the
    /// upload of `ios-v0.1.0-rc.1` fail for want of a compliance code.
    func testTheAppAnswersExportCompliance() {
        let value = Bundle.main.object(forInfoDictionaryKey: "ITSAppUsesNonExemptEncryption") as? Bool
        XCTAssertEqual(value, false)
    }

    func testTheAppShipsItsPrivacyManifest() throws {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "PrivacyInfo", withExtension: "xcprivacy"))
        let plist = try PropertyListSerialization.propertyList(from: Data(contentsOf: url), format: nil)
        let manifest = try XCTUnwrap(plist as? [String: Any])
        XCTAssertEqual(manifest["NSPrivacyTracking"] as? Bool, false)
        XCTAssertEqual((manifest["NSPrivacyCollectedDataTypes"] as? [Any])?.count, 0)
        let reasons = (manifest["NSPrivacyAccessedAPITypes"] as? [[String: Any]] ?? [])
            .compactMap { $0["NSPrivacyAccessedAPIType"] as? String }
        XCTAssertTrue(reasons.contains("NSPrivacyAccessedAPICategoryUserDefaults"))
    }

    /// Apple checks each binary in the bundle against its own manifest.
    func testEveryEmbeddedBundleShipsAPrivacyManifest() {
        let app = Bundle.main.bundleURL
        for bundle in [
            "PlugIns/RestoreExtension.appex", "Frameworks/MdocFFI.framework", "Frameworks/pdfium.framework",
        ] {
            let manifest = app.appending(path: "\(bundle)/PrivacyInfo.xcprivacy")
            XCTAssertTrue(FileManager.default.fileExists(atPath: manifest.path), "\(bundle) has no privacy manifest")
        }
    }
}
