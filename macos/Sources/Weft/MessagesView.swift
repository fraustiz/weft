// A phone's SMS conversations, threads and contacts (docs/features.md
// section 7), presented as a sheet from the device's page or from an
// incoming SMS notification. Nothing here is written to disk.

import AppKit
import SwiftUI
import WeftFFI
import WeftKit

struct MessagesSheet: View {
    @ObservedObject var model: AppModel
    let deviceId: String

    @StateObject private var composing = Local(false)
    @StateObject private var recipient = Local("")
    @StateObject private var draft = Local("")
    /// Addresses of a newly started conversation until the phone assigns a thread id.
    @StateObject private var pendingAddresses = Local<[String]?>(nil)
    @FocusState private var replyFocused: Bool

    private var device: DeviceInfo? { model.snapshot?.device(deviceId) }

    var body: some View {
        Group {
            if let device {
                content(device)
            } else {
                Color.clear
                    .onAppear { model.closeMessagesSheet() }
            }
        }
        .frame(minWidth: 680, idealWidth: 740, minHeight: 480, idealHeight: 540)
    }

    private func content(_ device: DeviceInfo) -> some View {
        HStack(spacing: 0) {
            sidebar(device)
                .frame(width: 248)
            Divider()
            detailPane(device)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .onAppear {
            selectInitialThreadIfNeeded(device)
        }
        .onChange(of: device.sms.conversations) { _, _ in
            adoptThreadForPendingIfAvailable(device)
            selectInitialThreadIfNeeded(device)
        }
    }

    // MARK: Conversations sidebar

    private func sidebar(_ device: DeviceInfo) -> some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(t("Messages", "Messages"))
                        .font(.headline)
                    Text(device.displayName)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
                if device.takes("sms.send") {
                    Button {
                        startNewMessage()
                    } label: {
                        Image(systemName: "square.and.pencil")
                            .frame(width: 24, height: 24)
                    }
                    .buttonStyle(.borderless)
                    .help(t("New Message", "Nouveau message"))
                    .accessibilityLabel(t("New Message", "Nouveau message"))
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            Divider()
            conversationsList(device)
            Divider()
            HStack {
                Button(t("Close", "Fermer")) {
                    model.closeMessagesSheet()
                }
                .keyboardShortcut(.cancelAction)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
        }
    }

    @ViewBuilder
    private func conversationsList(_ device: DeviceInfo) -> some View {
        if !device.sms.loaded {
            VStack(spacing: 8) {
                ProgressView().controlSize(.small)
                Text(t("Waiting for conversations…", "En attente des conversations…"))
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            .padding(20)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if device.sms.conversations.isEmpty {
            VStack(spacing: 8) {
                Text(t("No conversations", "Aucune conversation"))
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            .padding(20)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollView {
                LazyVStack(spacing: 4) {
                    ForEach(device.sms.conversations, id: \.thread) { conversation in
                        let selected = !composing.value
                            && pendingAddresses.value == nil
                            && model.selectedThread == conversation.thread
                        ConversationRow(conversation: conversation, selected: selected) {
                            chooseConversation(conversation.thread, on: device)
                        }
                    }
                }
                .padding(8)
            }
        }
    }

    // MARK: Detail pane

    @ViewBuilder
    private func detailPane(_ device: DeviceInfo) -> some View {
        if composing.value {
            newMessagePane(device)
        } else if let addresses = pendingAddresses.value {
            threadPane(device: device, threadId: nil, addresses: addresses, title: addresses.joined(separator: ", "))
        } else if let threadId = model.selectedThread {
            let conversation = device.smsConversation(threadId)
            let addresses = conversation?.addresses ?? []
            let title = conversation?.title ?? addresses.joined(separator: ", ")
            threadPane(device: device, threadId: threadId, addresses: addresses, title: title)
        } else {
            VStack(spacing: 12) {
                Image(systemName: "bubble.left.and.bubble.right")
                    .font(.system(size: 28, weight: .light))
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                Text(t("Select a conversation", "Sélectionnez une conversation"))
                    .foregroundStyle(.secondary)
                if device.takes("sms.send") {
                    Button(t("New Message", "Nouveau message")) {
                        startNewMessage()
                    }
                }
            }
            .padding(24)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func newMessagePane(_ device: DeviceInfo) -> some View {
        let contacts = device.sms.contacts.map { SmsContact(name: $0.name, numbers: $0.numbers) }
        let matches = SmsFormatting.suggestions(contacts: contacts, query: recipient.value)
        let resolved = SmsFormatting.recipient(contacts: contacts, text: recipient.value)

        return VStack(spacing: 0) {
            HStack {
                Text(t("New Message", "Nouveau message"))
                    .font(.headline)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 12)
            Divider()
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 8) {
                    Text(t("To:", "À :"))
                        .foregroundStyle(.secondary)
                    TextField(
                        t("Recipient", "Destinataire"),
                        text: $recipient.value,
                        prompt: Text(t("Contact name or phone number", "Nom d’un contact ou numéro"))
                    )
                    .textFieldStyle(.roundedBorder)
                    .labelsHidden()
                }
                Text(recipientHint(contacts: contacts, resolved: resolved))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                if !matches.isEmpty && resolved == nil {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(matches) { suggestion in
                            Button {
                                recipient.value = suggestion.label
                                replyFocused = true
                            } label: {
                                HStack {
                                    Text(suggestion.label)
                                        .lineLimit(1)
                                    Spacer(minLength: 0)
                                }
                                .padding(.horizontal, 8)
                                .padding(.vertical, 6)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding(4)
                    .background(
                        RoundedRectangle(cornerRadius: 8, style: .continuous)
                            .fill(Color(nsColor: .controlBackgroundColor))
                    )
                    .overlay(
                        RoundedRectangle(cornerRadius: 8, style: .continuous)
                            .strokeBorder(Color(nsColor: .separatorColor), lineWidth: 1)
                    )
                }
                Spacer(minLength: 0)
            }
            .padding(20)
            Divider()
            composeBar(
                canSend: resolved != nil && !draft.value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && device.takes("sms.send")
            ) {
                sendNewMessage(on: device, contacts: contacts)
            }
        }
    }

    private func threadPane(
        device: DeviceInfo,
        threadId: Int64?,
        addresses: [String],
        title: String
    ) -> some View {
        let loaded = threadId.flatMap { device.smsThread($0) }
        let messages = loaded?.messages ?? []
        let complete = threadId == nil ? true : (loaded?.complete ?? false)
        let sending = device.sms.sending.filter { sent in
            if let own = sent.thread, let shown = threadId {
                return own == shown
            }
            return SmsFormatting.samePeople(sent.addresses, addresses)
        }
        let subtitle = addresses.joined(separator: ", ")
        let canReply = device.takes("sms.send") && !addresses.isEmpty

        return VStack(spacing: 0) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title.isEmpty ? t("Conversation", "Conversation") : title)
                        .font(.headline)
                        .lineLimit(1)
                    if !subtitle.isEmpty && subtitle != title {
                        Text(subtitle)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 12)
            Divider()
            ScrollViewReader { proxy in
                ScrollView {
                    VStack(spacing: 12) {
                        if let threadId, !complete && !messages.isEmpty {
                            Button(t("Load Earlier", "Messages plus anciens")) {
                                model.loadEarlierMessages(
                                    thread: threadId,
                                    before: messages.first?.timeMs,
                                    on: device
                                )
                            }
                            .controlSize(.small)
                        }
                        if threadId != nil && messages.isEmpty && !complete {
                            HStack(spacing: 8) {
                                ProgressView().controlSize(.small)
                                Text(t("Loading…", "Chargement…"))
                                    .font(.callout)
                                    .foregroundStyle(.secondary)
                            }
                            .padding(.vertical, 16)
                        } else if messages.isEmpty && sending.isEmpty && complete {
                            Text(t("No messages yet", "Pas encore de message"))
                                .font(.callout)
                                .foregroundStyle(.secondary)
                                .padding(.vertical, 24)
                        }
                        VStack(spacing: 8) {
                            ForEach(Array(messages.enumerated()), id: \.element.id) { index, message in
                                let next = index + 1 < messages.count ? messages[index + 1] : nil
                                let previous = index > 0 ? messages[index - 1] : nil
                                let showSender = !message.outgoing
                                    && addresses.count > 1
                                    && (previous?.outgoing != false || previous?.address != message.address)
                                let lastOfRun = SmsFormatting.isLastOfRun(
                                    timeMs: message.timeMs,
                                    outgoing: message.outgoing,
                                    nextTimeMs: next?.timeMs,
                                    nextOutgoing: next?.outgoing
                                )
                                MessageBubbleRow(
                                    text: message.body,
                                    outgoing: message.outgoing,
                                    sender: showSender ? (message.sender ?? message.address) : nil,
                                    meta: bubbleMeta(for: message, lastOfRun: lastOfRun)
                                )
                            }
                            ForEach(sending, id: \.requestId) { item in
                                MessageBubbleRow(
                                    text: item.text,
                                    outgoing: true,
                                    sender: nil,
                                    meta: sendingMeta(for: item)
                                )
                            }
                        }
                        Color.clear
                            .frame(height: 1)
                            .id("thread-bottom")
                    }
                    .padding(.horizontal, 20)
                    .padding(.vertical, 16)
                }
                .onAppear {
                    proxy.scrollTo("thread-bottom", anchor: .bottom)
                    replyFocused = true
                }
                .onChange(of: threadId) { _, _ in
                    proxy.scrollTo("thread-bottom", anchor: .bottom)
                    replyFocused = true
                }
                .onChange(of: messages.last?.id) { _, _ in
                    proxy.scrollTo("thread-bottom", anchor: .bottom)
                }
                .onChange(of: sending.count) { _, _ in
                    proxy.scrollTo("thread-bottom", anchor: .bottom)
                }
            }
            if canReply {
                Divider()
                composeBar(
                    canSend: !draft.value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ) {
                    sendInThread(threadId: threadId, addresses: addresses, on: device)
                }
            }
        }
    }

    private func composeBar(canSend: Bool, send: @escaping () -> Void) -> some View {
        HStack(alignment: .bottom, spacing: 8) {
            TextField(
                t("Message", "Message"),
                text: $draft.value,
                prompt: Text(t("Text message", "SMS")),
                axis: .vertical
            )
            .lineLimit(1...5)
            .textFieldStyle(.roundedBorder)
            .focused($replyFocused)
            .labelsHidden()
            .onSubmit {
                if canSend { send() }
            }
            Button(t("Send", "Envoyer")) {
                send()
            }
            .buttonStyle(WeftProminentButtonStyle())
            .disabled(!canSend)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 12)
    }

    // MARK: Helpers

    private func selectInitialThreadIfNeeded(_ device: DeviceInfo) {
        guard !composing.value,
              pendingAddresses.value == nil,
              model.selectedThread == nil,
              let first = device.sms.conversations.first else {
            return
        }
        model.openThread(first.thread, on: device)
    }

    private func adoptThreadForPendingIfAvailable(_ device: DeviceInfo) {
        guard let addresses = pendingAddresses.value,
              let match = device.sms.conversations.first(where: {
                  SmsFormatting.samePeople($0.addresses, addresses)
              }) else {
            return
        }
        pendingAddresses.value = nil
        model.openThread(match.thread, on: device)
    }

    private func startNewMessage() {
        composing.value = true
        pendingAddresses.value = nil
        recipient.value = ""
        draft.value = ""
    }

    private func chooseConversation(_ thread: Int64, on device: DeviceInfo) {
        composing.value = false
        pendingAddresses.value = nil
        draft.value = ""
        model.openThread(thread, on: device)
    }

    private func sendNewMessage(on device: DeviceInfo, contacts: [SmsContact]) {
        let text = draft.value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty,
              let number = SmsFormatting.recipient(contacts: contacts, text: recipient.value) else {
            return
        }
        let addresses = [number]
        let existing = device.sms.conversations.first {
            SmsFormatting.samePeople($0.addresses, addresses)
        }
        composing.value = false
        draft.value = ""
        if let existing {
            pendingAddresses.value = nil
            model.selectedThread = existing.thread
            model.sendSms(addresses: addresses, text: text, thread: existing.thread, on: device)
        } else {
            pendingAddresses.value = addresses
            model.selectedThread = nil
            model.sendSms(addresses: addresses, text: text, thread: nil, on: device)
        }
    }

    private func sendInThread(threadId: Int64?, addresses: [String], on device: DeviceInfo) {
        let text = draft.value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !addresses.isEmpty else { return }
        draft.value = ""
        model.sendSms(addresses: addresses, text: text, thread: threadId, on: device)
    }

    private func recipientHint(contacts: [SmsContact], resolved: String?) -> String {
        let trimmed = recipient.value.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty && resolved == nil {
            return t("Choose a contact below or enter a phone number.",
                     "Choisissez un contact ci-dessous ou saisissez un numéro.")
        }
        if contacts.isEmpty {
            return t("Enter a phone number. Contacts appear once the phone sends them.",
                     "Saisissez un numéro. Les contacts s’affichent dès que le téléphone les envoie.")
        }
        return t("Type a contact’s name or a phone number.",
                 "Saisissez le nom d’un contact ou un numéro.")
    }

    private func bubbleMeta(for message: SmsMessageInfo, lastOfRun: Bool) -> (text: String, failed: Bool)? {
        if message.status == "failed" {
            return (t("Not sent", "Non envoyé"), true)
        }
        if message.status == "pending" {
            return (t("Sending…", "Envoi…"), false)
        }
        guard lastOfRun else { return nil }
        let stamp = SmsFormatting.timestamp(ms: message.timeMs, alwaysIncludeTime: true)
        return stamp.isEmpty ? nil : (stamp, false)
    }

    private func sendingMeta(for item: SendingSmsInfo) -> (text: String, failed: Bool) {
        switch item.state {
        case .pending:
            return (t("Sending…", "Envoi…"), false)
        case .sent:
            return (t("Sent", "Envoyé"), false)
        case .failed(let error):
            let reason = SmsFormatting.failureText(error)
            let line = Language.current == .french
                ? "Non envoyé : \(reason)"
                : "Not sent: \(reason)"
            return (line, true)
        }
    }
}

private struct ConversationRow: View {
    let conversation: ConversationInfo
    let selected: Bool
    let select: () -> Void

    var body: some View {
        Button {
            select()
        } label: {
            HStack(alignment: .top, spacing: 8) {
                Circle()
                    .fill(conversation.unread ? (selected ? Color.weftInk : Color.weftOrange) : Color.clear)
                    .frame(width: 8, height: 8)
                    .padding(.top, 5)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 2) {
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Text(conversation.title)
                            .fontWeight(conversation.unread ? .semibold : .regular)
                            .lineLimit(1)
                        Spacer(minLength: 0)
                        let stamp = SmsFormatting.timestamp(ms: conversation.timeMs)
                        if !stamp.isEmpty {
                            Text(stamp)
                                .font(.caption)
                                .monospacedDigit()
                                .foregroundStyle(selected ? AnyShapeStyle(Color.weftInk.opacity(0.75)) : AnyShapeStyle(.secondary))
                        }
                    }
                    if let snippet = conversation.snippet, !snippet.isEmpty {
                        Text(snippet)
                            .font(.caption)
                            .foregroundStyle(selected ? AnyShapeStyle(Color.weftInk.opacity(0.8)) : AnyShapeStyle(.secondary))
                            .lineLimit(1)
                    }
                }
            }
            .foregroundStyle(selected ? Color.weftInk : Color.primary)
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(selected ? Color.weftOrange : Color.clear)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

private struct MessageBubbleRow: View {
    let text: String
    let outgoing: Bool
    let sender: String?
    let meta: (text: String, failed: Bool)?

    var body: some View {
        HStack {
            if outgoing { Spacer(minLength: 64) }
            VStack(alignment: outgoing ? .trailing : .leading, spacing: 4) {
                if let sender, !sender.isEmpty {
                    Text(sender)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Text(text)
                    .foregroundStyle(outgoing ? Color.weftInk : Color.primary)
                    .textSelection(.enabled)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .background(
                        RoundedRectangle(cornerRadius: 12, style: .continuous)
                            .fill(outgoing ? Color.weftOrange : Color(nsColor: .controlBackgroundColor))
                    )
                    .overlay {
                        if !outgoing {
                            RoundedRectangle(cornerRadius: 12, style: .continuous)
                                .strokeBorder(Color(nsColor: .separatorColor), lineWidth: 1)
                        }
                    }
                if let meta {
                    Text(meta.text)
                        .font(.caption)
                        .monospacedDigit()
                        .foregroundStyle(meta.failed ? AnyShapeStyle(.red) : AnyShapeStyle(.secondary))
                }
            }
            if !outgoing { Spacer(minLength: 64) }
        }
    }
}
