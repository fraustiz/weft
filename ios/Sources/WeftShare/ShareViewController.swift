import Social
import UIKit
import UniformTypeIdentifiers

/// The extension stages shared content. Open Weft to select the destination;
/// credentials and a persistent relay connection stay in the containing app.
@MainActor
final class ShareViewController: SLComposeServiceViewController {
    private var french: Bool { Locale.preferredLanguages.first?.hasPrefix("fr") == true }

    override func isContentValid() -> Bool {
        contentText.utf8.count <= 256 * 1024
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        title = "Weft"
        placeholder = french ? "Ouvrez ensuite Weft pour choisir l’appareil." : "Then open Weft to choose a device."
    }

    nonisolated private static func stage(_ source: URL, in inbox: URL) throws -> URL {
        let scoped = source.startAccessingSecurityScopedResource()
        defer { if scoped { source.stopAccessingSecurityScopedResource() } }
        let size = try source.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey])
        guard size.isRegularFile == true, let bytes = size.fileSize,
              bytes <= 100 * 1024 * 1024 else { throw CocoaError(.fileReadTooLarge) }
        let target = inbox.appendingPathComponent(UUID().uuidString + "_" + source.lastPathComponent)
        try FileManager.default.copyItem(at: source, to: target)
        return target
    }

    override func didSelectPost() {
        Task { @MainActor in
            var staged: [URL] = []
            do {
                var text = contentText ?? ""
                let items = extensionContext?.inputItems as? [NSExtensionItem] ?? []
                let providers = items.flatMap({ $0.attachments ?? [] })
                guard let group = FileManager.default.containerURL(
                    forSecurityApplicationGroupIdentifier: "group.eu.frohub.weft") else {
                    throw CocoaError(.fileWriteUnknown)
                }
                let inbox = group.appendingPathComponent("ShareInbox", isDirectory: true)
                try FileManager.default.createDirectory(at: inbox, withIntermediateDirectories: true)
                let existing = try FileManager.default.contentsOfDirectory(at: inbox,
                    includingPropertiesForKeys: [.fileSizeKey])
                var total = try existing.reduce(0) { $0 + (try $1.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0) }
                guard existing.count + max(1, providers.count) <= 20, total < 200 * 1024 * 1024,
                      providers.count <= 10 else { throw CocoaError(.fileWriteOutOfSpace) }
                let files = providers.filter {
                    $0.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier)
                    || $0.hasItemConformingToTypeIdentifier(UTType.image.identifier)
                    || (!$0.hasItemConformingToTypeIdentifier(UTType.url.identifier)
                        && !$0.hasItemConformingToTypeIdentifier(UTType.text.identifier))
                }
                for provider in files {
                    guard let type = provider.registeredTypeIdentifiers.first(where: {
                        UTType($0)?.conforms(to: .data) == true && $0 != UTType.fileURL.identifier
                    }) ?? provider.registeredTypeIdentifiers.first else { continue }
                    let file: URL = try await withCheckedThrowingContinuation { continuation in
                        if type == UTType.fileURL.identifier {
                            provider.loadItem(forTypeIdentifier: type, options: nil) { value, error in
                                do {
                                    guard let source = value as? URL else { throw error ?? CocoaError(.fileReadUnknown) }
                                    continuation.resume(returning: try Self.stage(source, in: inbox))
                                } catch { continuation.resume(throwing: error) }
                            }
                        } else {
                            provider.loadFileRepresentation(forTypeIdentifier: type) { source, error in
                                do {
                                    guard let source else { throw error ?? CocoaError(.fileReadUnknown) }
                                    continuation.resume(returning: try Self.stage(source, in: inbox))
                                } catch { continuation.resume(throwing: error) }
                            }
                        }
                    }
                    staged.append(file)
                    total += (try file.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0)
                    guard total <= 200 * 1024 * 1024 else { throw CocoaError(.fileWriteOutOfSpace) }
                }
                if !files.isEmpty {
                    extensionContext?.completeRequest(returningItems: nil)
                    return
                }
                if let provider = providers.first(where: {
                    $0.hasItemConformingToTypeIdentifier(UTType.url.identifier)
                }) {
                    let urlText: String? = await withCheckedContinuation { continuation in
                        provider.loadItem(forTypeIdentifier: UTType.url.identifier, options: nil) { value, _ in
                            continuation.resume(returning: (value as? URL)?.absoluteString)
                        }
                    }
                    if let urlText { text = urlText }
                }
                guard !text.isEmpty, text.utf8.count <= 256 * 1024 else {
                    throw CocoaError(.fileWriteUnknown)
                }
                try Data(text.utf8).write(to: inbox.appendingPathComponent(UUID().uuidString + ".wefttext"), options: .atomic)
                extensionContext?.completeRequest(returningItems: nil)
            } catch {
                for file in staged { try? FileManager.default.removeItem(at: file) }
                let alert = UIAlertController(title: french ? "Partage impossible" : "Could not save share",
                    message: french ? "Ouvrez Weft et réessayez." : "Open Weft and try again.", preferredStyle: .alert)
                alert.addAction(UIAlertAction(title: "OK", style: .default))
                present(alert, animated: true)
            }
        }
    }
}
