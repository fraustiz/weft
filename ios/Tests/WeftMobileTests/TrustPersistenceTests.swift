import Foundation
import Testing
import WeftProtocol
@testable import WeftMobileKit

@Test func failedTrustRemovalKeepsInMemoryTrustConsistentWithDisk() throws {
    let fm = FileManager.default
    let root = fm.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? fm.removeItem(at: root) }
    let directory = root.appendingPathComponent("state")
    let store = try MobileStore(fileURL: directory.appendingPathComponent("state.json"))
    let identity = try Identity(signSeed: Data(repeating: 1, count: 32), kxSeed: Data(repeating: 2, count: 32))
    let keys = try PeerKeys(signPublicKey: identity.signPublicKey, kxPublicKey: identity.kxPublicKey)
    _ = try store.trustPeer(keys)
    // Force an actual persistence failure independently of user/root permissions.
    try fm.removeItem(at: directory)
    try Data("not a directory".utf8).write(to: directory)
    var failed = false
    do { try store.untrustPeer(keys.deviceID.hexString) }
    catch { failed = true }
    #expect(failed)
    #expect(store.trustedPeer(keys.deviceID.hexString) != nil)
    failed = false
    do { try store.clearGroup() }
    catch { failed = true }
    #expect(failed)
    #expect(store.trustedPeers.count == 1)
}
