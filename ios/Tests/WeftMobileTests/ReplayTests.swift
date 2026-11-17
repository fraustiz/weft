import Foundation
import Testing
import WeftProtocol
@testable import WeftMobileKit

@Test func replayRejectsCounterAndMessageReuseAcrossPersistence() throws {
    let id = Data(repeating: 1, count: 16)
    var replay = ReplayWindow()
    try replay.accept(counter: 1, messageID: id)
    let restored = try JSONDecoder().decode(ReplayWindow.self, from: JSONEncoder().encode(replay))
    #expect(restored.check(counter: 2, messageID: id) == .duplicate)
    #expect(restored.check(counter: 1, messageID: Data(repeating: 2, count: 16)) == .duplicate)
    #expect(restored.check(counter: 0, messageID: id) == .zeroCounter)
    try replay.accept(counter: 1025, messageID: Data(repeating: 3, count: 16))
    #expect(replay.check(counter: 1, messageID: id) == .tooOld)
}

@Test func joinBufferBoundsUntrustedTrafficAndDrainsInOrder() {
    let buffer = JoinFrameBuffer(ownerHex: "OWNER", maxDeferred: 2)
    for byte in [UInt8(1), 2, 3] {
        #expect(buffer.onEnvelope(senderHex: "other", kind: .welcome, isTrusted: false,
                                  frame: Data([byte])) == .deferred)
    }
    #expect(buffer.bufferedCount == 2)
    #expect(buffer.onEnvelope(senderHex: "owner", kind: .welcome, isTrusted: false,
                              frame: Data()) == .welcome)
    #expect(buffer.settleAndDrain() == [Data([1]), Data([2])])
    #expect(buffer.settleAndDrain().isEmpty)
    #expect(buffer.onEnvelope(senderHex: "other", kind: .welcome, isTrusted: false,
                              frame: Data()) == .processNow)
}
