import Foundation
import Security

/// Where the 32-byte key sealing saved cases lives (ADR 0085): made on first
/// use, never leaving the device.
protocol CaseKeyStore: Sendable {
    /// The key, made and stored the first time.
    func key() throws -> Data
}

enum CaseKeyError: Error, LocalizedError, Equatable {
    case keychain(OSStatus)
    case random(OSStatus)

    var errorDescription: String? {
        switch self {
        case .keychain(let status):
            String(localized: "mdoc could not read its case key from the Keychain (\(Int(status))).")
        case .random(let status):
            String(localized: "mdoc could not make a case key (\(Int(status))).")
        }
    }
}

/// The case key as a Keychain item the app and the Restore extension share.
/// It goes to the first group of `keychain-access-groups` (the shared one),
/// readable only while the device is unlocked and never synced or restored
/// to another device.
struct KeychainCaseKeyStore: CaseKeyStore {
    var service = "io.github.lawkitt.mdoc"
    var account = "case-key"

    func key() throws -> Data {
        if let existing = try read() {
            return existing
        }
        var bytes = Data(count: 32)
        let status = bytes.withUnsafeMutableBytes { buffer in
            SecRandomCopyBytes(kSecRandomDefault, 32, buffer.baseAddress!)
        }
        guard status == errSecSuccess else { throw CaseKeyError.random(status) }
        let add: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
            kSecAttrSynchronizable as String: false,
            kSecValueData as String: bytes,
        ]
        let added = SecItemAdd(add as CFDictionary, nil)
        if added == errSecDuplicateItem, let existing = try read() {
            // The extension made it first.
            return existing
        }
        guard added == errSecSuccess else { throw CaseKeyError.keychain(added) }
        return bytes
    }

    private func read() throws -> Data? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        switch status {
        case errSecSuccess:
            guard let data = result as? Data, data.count == 32 else { throw CaseKeyError.keychain(errSecDecode) }
            return data
        case errSecItemNotFound:
            return nil
        default:
            throw CaseKeyError.keychain(status)
        }
    }

    /// Removes the item (tests only).
    func remove() {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(query as CFDictionary)
    }
}

/// A fixed key for tests and previews.
struct FixedCaseKeyStore: CaseKeyStore {
    var bytes = Data(repeating: 42, count: 32)

    func key() throws -> Data { bytes }
}
