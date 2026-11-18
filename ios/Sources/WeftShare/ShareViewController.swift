import ImageIO
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// Same key as `SharedLanguage.key` in WeftMobileKit: the app writes its
/// resolved language ("en" or "fr") there at launch, because the per-app
/// language setting does not apply to the extension.
private let appLanguageKey = "WeftAppLanguage"

/// The app's language when it has been written; otherwise the same rule as
/// `Language.detect` in WeftMobileKit: the first preferred language that is
/// English or French wins.
private let prefersFrench: Bool = {
    switch UserDefaults(suiteName: "group.eu.frohub.weft")?.string(forKey: appLanguageKey) {
    case "fr": return true
    case "en": return false
    default:
        return Locale.preferredLanguages.lazy
            .map { $0.lowercased() }
            .first { $0.hasPrefix("fr") || $0.hasPrefix("en") }?
            .hasPrefix("fr") == true
    }
}()

private func t(_ english: String, _ french: String) -> String {
    prefersFrench ? french : english
}

private let weftOrange = Color(red: 0xF2 / 255.0, green: 0x6A / 255.0, blue: 0x21 / 255.0)
private let weftInk = Color(red: 0x17 / 255.0, green: 0x13 / 255.0, blue: 0x0F / 255.0)

/// The extension stages shared content. Open Weft to select the destination;
/// credentials and a persistent relay connection stay in the containing app.
@MainActor
final class ShareViewController: UIViewController {
    private let sheet = ShareSheetState()

    override func viewDidLoad() {
        super.viewDidLoad()
        let host = UIHostingController(rootView: ShareSheetView(
            state: sheet,
            onSend: { [weak self] in self?.didSelectPost() },
            onCancel: { [weak self] in self?.cancel() }
        ))
        addChild(host)
        host.view.frame = view.bounds
        host.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(host.view)
        host.didMove(toParent: self)
        Task { sheet.preview = await loadPreview() }
    }

    private var providers: [NSItemProvider] {
        let items = extensionContext?.inputItems as? [NSExtensionItem] ?? []
        return items.flatMap({ $0.attachments ?? [] })
    }

    /// Text the host app attached outside the attachments (what the old
    /// compose sheet showed in its text view).
    private var contentText: String {
        let items = extensionContext?.inputItems as? [NSExtensionItem] ?? []
        return items.lazy.compactMap { $0.attributedContentText?.string }.first ?? ""
    }

    private func cancel() {
        extensionContext?.cancelRequest(withError: CocoaError(.userCancelled))
    }

    // MARK: - Preview

    private func loadPreview() async -> SharePreview {
        let providers = providers
        let files = Self.fileProviders(providers)
        if let first = files.first {
            if first.hasItemConformingToTypeIdentifier(UTType.image.identifier),
               let image = await Self.thumbnail(first) {
                return .image(image, count: files.count)
            }
            return .file(first.suggestedName ?? t("File", "Fichier"), count: files.count)
        }
        if let provider = providers.first(where: {
            $0.hasItemConformingToTypeIdentifier(UTType.url.identifier)
        }), let url = await Self.loadURL(provider) {
            return .text(url, isLink: true)
        }
        var text = contentText
        if text.isEmpty, let provider = providers.first(where: {
            $0.hasItemConformingToTypeIdentifier(UTType.text.identifier)
        }) {
            text = await Self.loadText(provider) ?? ""
        }
        return .text(text, isLink: false)
    }

    private static func thumbnail(_ provider: NSItemProvider) async -> UIImage? {
        await withCheckedContinuation { continuation in
            provider.loadFileRepresentation(forTypeIdentifier: UTType.image.identifier) { source, _ in
                let options = [
                    kCGImageSourceCreateThumbnailFromImageAlways: true,
                    kCGImageSourceCreateThumbnailWithTransform: true,
                    kCGImageSourceThumbnailMaxPixelSize: 800,
                ] as CFDictionary
                guard let source, let image = CGImageSourceCreateWithURL(source as CFURL, nil),
                      let thumbnail = CGImageSourceCreateThumbnailAtIndex(image, 0, options) else {
                    continuation.resume(returning: nil)
                    return
                }
                continuation.resume(returning: UIImage(cgImage: thumbnail))
            }
        }
    }

    // MARK: - Queueing (`group.eu.frohub.weft`/ShareInbox)

    private static func fileProviders(_ providers: [NSItemProvider]) -> [NSItemProvider] {
        providers.filter {
            $0.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier)
            || $0.hasItemConformingToTypeIdentifier(UTType.image.identifier)
            || (!$0.hasItemConformingToTypeIdentifier(UTType.url.identifier)
                && !$0.hasItemConformingToTypeIdentifier(UTType.text.identifier))
        }
    }

    private static func loadURL(_ provider: NSItemProvider) async -> String? {
        await withCheckedContinuation { continuation in
            provider.loadItem(forTypeIdentifier: UTType.url.identifier, options: nil) { value, _ in
                continuation.resume(returning: (value as? URL)?.absoluteString)
            }
        }
    }

    private static func loadText(_ provider: NSItemProvider) async -> String? {
        let type = provider.hasItemConformingToTypeIdentifier(UTType.plainText.identifier)
            ? UTType.plainText.identifier : UTType.text.identifier
        return await withCheckedContinuation { continuation in
            provider.loadItem(forTypeIdentifier: type, options: nil) { value, _ in
                let decoded: String?
                if let string = value as? String { decoded = string }
                else if let attributed = value as? NSAttributedString { decoded = attributed.string }
                else if let data = value as? Data { decoded = String(data: data, encoding: .utf8) }
                else { decoded = nil }
                continuation.resume(returning: decoded)
            }
        }
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

    private func didSelectPost() {
        sheet.sending = true
        Task { @MainActor in
            var staged: [URL] = []
            do {
                var text = contentText
                let providers = providers
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
                let files = Self.fileProviders(providers)
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
                }), let urlText = await Self.loadURL(provider) {
                    text = urlText
                }
                if text.isEmpty, let provider = providers.first(where: {
                    $0.hasItemConformingToTypeIdentifier(UTType.text.identifier)
                }), let suppliedText = await Self.loadText(provider) {
                    text = suppliedText
                }
                guard !text.isEmpty, text.utf8.count <= 256 * 1024 else {
                    throw CocoaError(.fileWriteUnknown)
                }
                guard total + text.utf8.count <= 200 * 1024 * 1024 else {
                    throw CocoaError(.fileWriteOutOfSpace)
                }
                try Data(text.utf8).write(to: inbox.appendingPathComponent(UUID().uuidString + ".wefttext"), options: .atomic)
                extensionContext?.completeRequest(returningItems: nil)
            } catch {
                for file in staged { try? FileManager.default.removeItem(at: file) }
                sheet.sending = false
                let alert = UIAlertController(title: t("Could not save share", "Partage impossible"),
                    message: t("Open Weft and try again.", "Ouvrez Weft et réessayez."), preferredStyle: .alert)
                alert.addAction(UIAlertAction(title: "OK", style: .default))
                present(alert, animated: true)
            }
        }
    }
}

// MARK: - Sheet

enum SharePreview {
    case loading
    case image(UIImage, count: Int)
    case file(String, count: Int)
    case text(String, isLink: Bool)
}

@MainActor
final class ShareSheetState: ObservableObject {
    @Published var preview = SharePreview.loading
    @Published var sending = false
}

private struct ShareSheetView: View {
    @ObservedObject var state: ShareSheetState
    let onSend: () -> Void
    let onCancel: () -> Void

    var body: some View {
        NavigationStack {
            VStack(spacing: 16) {
                previewCard
                Text(t("Weft sends it from the app, where you choose the device.",
                       "Weft l’enverra depuis l’app, où vous choisirez l’appareil."))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: .infinity)
                Spacer(minLength: 0)
                Button(action: onSend) {
                    ZStack {
                        Text(t("Add to Weft", "Ajouter à Weft"))
                            .opacity(state.sending ? 0 : 1)
                        if state.sending {
                            ProgressView().tint(weftInk)
                        }
                    }
                }
                .buttonStyle(PrimaryButtonStyle())
                .disabled(state.sending || isLoading)
            }
            .padding(20)
            .background(Color(uiColor: .systemGroupedBackground).ignoresSafeArea())
            .navigationTitle("Weft")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(t("Cancel", "Annuler"), action: onCancel)
                        .tint(.primary)
                        .disabled(state.sending)
                }
            }
        }
        .tint(weftOrange)
    }

    private var isLoading: Bool {
        if case .loading = state.preview { return true }
        return false
    }

    @ViewBuilder
    private var previewCard: some View {
        let card = RoundedRectangle(cornerRadius: 16, style: .continuous)
        Group {
            switch state.preview {
            case .loading:
                ProgressView()
                    .frame(maxWidth: .infinity, minHeight: 96)
            case .image(let image, let count):
                VStack(spacing: 12) {
                    Image(uiImage: image)
                        .resizable()
                        .scaledToFit()
                        .frame(maxHeight: 320)
                        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                    if count > 1 {
                        countCaption(count)
                    }
                }
                .frame(maxWidth: .infinity)
            case .file(let name, let count):
                row(symbol: "doc") {
                    Text(name)
                        .lineLimit(2)
                        .truncationMode(.middle)
                    if count > 1 {
                        countCaption(count)
                    }
                }
            case .text(let text, let isLink):
                row(symbol: isLink ? "link" : "text.alignleft") {
                    Text(text)
                        .lineLimit(isLink ? 3 : 8)
                        .truncationMode(isLink ? .middle : .tail)
                }
            }
        }
        .padding(16)
        .background(card.fill(Color(uiColor: .secondarySystemGroupedBackground)))
    }

    private func row<Content: View>(symbol: String, @ViewBuilder content: () -> Content) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Image(systemName: symbol)
                .foregroundStyle(weftOrange)
                .frame(width: 24)
            VStack(alignment: .leading, spacing: 4) {
                content()
            }
            Spacer(minLength: 0)
        }
    }

    private func countCaption(_ count: Int) -> some View {
        Text(t("\(count) items", "\(count) éléments"))
            .font(.footnote)
            .foregroundStyle(.secondary)
            .monospacedDigit()
    }
}

/// Mirrors the app's `WeftProminentButtonStyle`: orange surface, ink text.
private struct PrimaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.body.weight(.medium))
            .foregroundStyle(weftInk)
            .frame(maxWidth: .infinity, minHeight: 24)
            .padding(.vertical, 12)
            .background(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(weftOrange)
                    .brightness(configuration.isPressed ? -0.07 : 0)
            )
            .opacity(isEnabled ? 1.0 : 0.45)
            .contentShape(Rectangle())
    }
}
