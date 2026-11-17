// The links a device may share (docs/features.md section 4): http and
// https only, with a host, at most 2048 bytes, no spaces. The same rule
// as the core's `is_shareable_url`, so a link field can say at once
// whether it will be sent.

import Foundation

public enum Links {
    /// The longest link sent, in bytes.
    public static let maxBytes = 2048

    public static func isShareable(_ url: String) -> Bool {
        let url = url.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !url.isEmpty, url.utf8.count <= maxBytes else { return false }
        if url.unicodeScalars.contains(where: {
            CharacterSet.whitespacesAndNewlines.contains($0) || CharacterSet.controlCharacters.contains($0)
        }) {
            return false
        }
        let lower = url.lowercased()
        let rest: Substring
        if lower.hasPrefix("https://") {
            rest = lower.dropFirst("https://".count)
        } else if lower.hasPrefix("http://") {
            rest = lower.dropFirst("http://".count)
        } else {
            return false
        }
        let authority = rest.split(omittingEmptySubsequences: false, whereSeparator: { "/?#".contains($0) })
            .first ?? ""
        let host = authority.split(separator: "@", omittingEmptySubsequences: false).last ?? ""
        return !host.isEmpty && !host.hasPrefix(":")
    }

    /// The text of the clipboard as a link to offer, when it is one.
    public static func candidate(fromClipboard text: String?) -> String? {
        guard let text = text?.trimmingCharacters(in: .whitespacesAndNewlines),
              isShareable(text) else { return nil }
        return text
    }

    /// A received link to open: a `URL` only for http and https.
    public static func openable(_ url: String) -> URL? {
        guard isShareable(url), let parsed = URL(string: url.trimmingCharacters(in: .whitespaces)),
              let scheme = parsed.scheme?.lowercased(), scheme == "http" || scheme == "https" else {
            return nil
        }
        return parsed
    }
}
