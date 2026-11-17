import Foundation
import Testing
@testable import WeftMobileKit

@Test func clipboardOffersAreBoundedAndExpire() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let engine = try TransferEngine(stagingDirectory: root.appendingPathComponent("stage"),
                                    destinationDirectory: root.appendingPathComponent("received"))
    func accepted(peer: String, id: Int, at now: Int64) -> Bool {
        let offer = ClipboardImageOfferPayload(id: String(format: "%032x", id), mime: "image/png",
            size: PacketConstants.maxClipboardImageSize, hash: String(repeating: "ab", count: 16))
        return engine.handlePacket(from: peer, packet: .clipboardImageOffer(offer), nowMs: now).contains {
            if case .sendPacket(_, .clipboardImageAccept(_)) = $0 { return true }
            return false
        }
    }
    #expect(accepted(peer: "a", id: 1, at: 0))
    for id in 2...100 { #expect(!accepted(peer: "a", id: id, at: 1)) }
    // Each accepted image reserves its full offered size against the global budget.
    let allowed = Int(TransferEngine.maxIncomingClipboardBytes / PacketConstants.maxClipboardImageSize)
    var admitted = 1
    for id in 101...120 {
        if accepted(peer: String(id), id: id, at: 1) { admitted += 1 }
    }
    #expect(admitted <= allowed)
    #expect(admitted <= TransferEngine.maxIncomingClipboardImages)
    _ = engine.tick(nowMs: TransferEngine.stallMs + 2)
    #expect(accepted(peer: "a", id: 200, at: TransferEngine.stallMs + 3))
}

@Test func capabilitiesAdvertiseImplementedMediaDirectionsOnly() {
    let capabilities = FeatureSwitches().capabilitiesPayload()
    #expect(!capabilities.incoming.contains("media.command"))
    #expect(!capabilities.incoming.contains("volume.set"))
    #expect(!capabilities.outgoing.contains("media"))
    #expect(!capabilities.outgoing.contains("media.art"))
    #expect(!capabilities.outgoing.contains("volume"))
}
