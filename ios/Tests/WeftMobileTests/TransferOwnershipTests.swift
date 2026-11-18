import Foundation
import Testing
@testable import WeftMobileKit

@Test func completedTransferCleansOwnedCopyButPreservesOriginal() throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let engine = try TransferEngine(stagingDirectory: root.appendingPathComponent("stage"),
                                    destinationDirectory: root.appendingPathComponent("received"))
    let original = root.appendingPathComponent("original.txt")
    let staged = engine.stagingDirectory.appendingPathComponent("outgoing_owned.txt")
    try Data("file content".utf8).write(to: original)
    try FileManager.default.copyItem(at: original, to: staged)
    let id = String(repeating: "ab", count: 16)
    let peer = String(repeating: "cd", count: 16)
    _ = try engine.offerFile(fileURL: staged, ownsFile: true, to: peer, transferIDHex: id)
    _ = engine.handlePacket(from: peer, packet: .fileDone(id: id))
    _ = engine.tick()
    #expect(!FileManager.default.fileExists(atPath: staged.path))
    #expect(FileManager.default.fileExists(atPath: original.path))
}
