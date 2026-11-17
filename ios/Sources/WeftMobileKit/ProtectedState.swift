#if os(iOS)
import CryptoKit
import Foundation
import Security

/// Device-bound Keychain key; neither state nor identity is recoverable from a backup.
enum ProtectedState {
    private static let marker = Data("WEFTSTATE1".utf8)

    private static func key(create: Bool) throws -> SymmetricKey {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "eu.frohub.weft.state",
            kSecAttrAccount as String: "encryption-key",
        ]
        var lookup = query
        lookup[kSecReturnData as String] = true
        lookup[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(lookup as CFDictionary, &result)
        if status == errSecSuccess, let data = result as? Data, data.count == 32 {
            return SymmetricKey(data: data)
        }
        guard status == errSecItemNotFound, create else {
            throw MobileStoreError.ioError("Device state key unavailable (\(status))")
        }
        let key = SymmetricKey(size: .bits256)
        var insert = query
        insert[kSecValueData as String] = key.withUnsafeBytes { Data($0) }
        insert[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let added = SecItemAdd(insert as CFDictionary, nil)
        guard added == errSecSuccess else {
            throw MobileStoreError.ioError("Could not protect device state (\(added))")
        }
        return key
    }

    static func seal(_ plaintext: Data) throws -> Data {
        let sealed = try AES.GCM.seal(plaintext, using: key(create: true), authenticating: marker)
        guard let combined = sealed.combined else { throw MobileStoreError.ioError("Could not seal state") }
        return marker + combined
    }

    static func open(_ data: Data) throws -> Data {
        // Initial development builds wrote protected plaintext. Migrate on load.
        guard data.starts(with: marker) else { return data }
        let box = try AES.GCM.SealedBox(combined: data.dropFirst(marker.count))
        return try AES.GCM.open(box, using: key(create: false), authenticating: marker)
    }
}
#endif
