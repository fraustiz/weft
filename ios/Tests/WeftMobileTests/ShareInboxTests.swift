import Foundation
import Testing
@testable import WeftMobileKit

@Test func shareInboxOffersTheOriginalFileName() {
    let id = "16BF01DC-0000-4000-8000-000000000000"
    #expect(ShareInbox.originalName(of: "\(id)_weft-share-test.PNG") == "weft-share-test.PNG")
    #expect(ShareInbox.originalName(of: "\(id)_my_file_v2.txt") == "my_file_v2.txt")
    // Names without the extension's prefix are left alone.
    #expect(ShareInbox.originalName(of: "notes_2026.txt") == "notes_2026.txt")
    #expect(ShareInbox.originalName(of: "\(id)_") == "\(id)_")
    #expect(ShareInbox.originalName(of: "photo.jpg") == "photo.jpg")
}

@Test func languageFollowsTheFirstEnglishOrFrenchPreference() {
    #expect(Language.detect(forced: nil, preferred: ["fr-FR", "en-US"]) == .french)
    #expect(Language.detect(forced: nil, preferred: ["de-DE", "fr-FR"]) == .french)
    #expect(Language.detect(forced: nil, preferred: ["en-GB", "fr-FR"]) == .english)
    #expect(Language.detect(forced: nil, preferred: ["es-ES"]) == .english)
    #expect(Language.detect(forced: "fr", preferred: ["en-US"]) == .french)
}
