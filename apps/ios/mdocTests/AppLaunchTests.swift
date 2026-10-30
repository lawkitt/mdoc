import XCTest
@testable import mdoc

/// A launch in the background can find the device locked: the Keychain
/// refuses the case key (`errSecInteractionNotAllowed`) and the core cannot
/// open. The app then shows why, and opens again once it is in front.
///
/// The Simulator cannot lock the device (XCUIDevice has no lock button and
/// its Keychain ignores the lock), so this drives the same failure through a
/// key store that refuses while "locked".
@MainActor
final class AppLaunchTests: XCTestCase {
    func testALockedLaunchOpensOnceTheAppIsInFront() throws {
        let storage = try Fixture.temporaryStorage()
        let keys = LockableKeyStore()
        let open = {
            (storage, try RealCore.make(storage: storage, keys: keys, physicalMemory: 4 * Fixture.gigabyte))
        }

        keys.locked = true
        let locked = AppModel.live(downloader: FakeModelDownloader(seconds: 0), open: open)
        XCTAssertNotNil(locked.startupError)
        XCTAssertTrue(locked.home.sections.isEmpty)

        // Still in the background, or still locked: nothing changes.
        XCTAssertTrue(locked.reopened(isActive: false) { AppModel.live(open: open) } === locked)

        keys.locked = false
        let reopened = locked.reopened(isActive: true) {
            AppModel.live(downloader: FakeModelDownloader(seconds: 0), open: open)
        }
        XCTAssertFalse(reopened === locked)
        XCTAssertNil(reopened.startupError)
        XCTAssertTrue(reopened.core.library is RealLibrary)

        // A model that opened is kept when the app comes back.
        XCTAssertTrue(reopened.reopened(isActive: true) { XCTFail("opened again"); return reopened } === reopened)
    }
}

/// A Keychain stand-in that refuses the key while "locked", as iOS does with
/// `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`.
private final class LockableKeyStore: CaseKeyStore, @unchecked Sendable {
    var locked = false
    private let bytes = Data(repeating: 7, count: 32)

    func key() throws -> Data {
        if locked {
            throw CaseKeyError.keychain(errSecInteractionNotAllowed)
        }
        return bytes
    }
}
