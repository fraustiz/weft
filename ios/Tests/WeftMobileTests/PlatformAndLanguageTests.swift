import Foundation
import Testing
@testable import WeftMobileKit

@Test func platformSymbolsDoNotAssumeHardware() {
    #expect(Words.symbol("macos") == "macwindow")
    #expect(Words.symbol("windows") == "pc")
    #expect(Words.symbol("linux") == "terminal")
    #expect(Words.symbol("android") == "iphone")
}

@Test func sharedLanguageCodes() {
    #expect(SharedLanguage.key == "WeftAppLanguage")
    #expect(SharedLanguage.code(.french) == "fr")
    #expect(SharedLanguage.code(.english) == "en")
}

@Test func offeredFileKeepsOriginalNameInsteadOfStagedName() throws {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("weft-name-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let engine = try TransferEngine(
        stagingDirectory: root.appendingPathComponent("staging", isDirectory: true),
        destinationDirectory: root.appendingPathComponent("dest", isDirectory: true)
    )
    let staged = engine.stagingDirectory.appendingPathComponent("outgoing_\(UUID().uuidString)_report.txt")
    try Data("x".utf8).write(to: staged)
    let actions = try engine.offerFile(
        fileURL: staged, name: "report.txt", ownsFile: true, to: String(repeating: "cd", count: 16)
    )
    #expect(engine.transfersSnapshot().map(\.name) == ["report.txt"])
    guard case .sendPacket(_, .fileOffer(let offer)) = actions.first else {
        Issue.record("expected a file offer"); return
    }
    #expect(offer.name == "report.txt")
}
