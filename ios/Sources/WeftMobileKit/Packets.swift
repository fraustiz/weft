// Wire packet models for envelope kind `0x07` (`docs/features.md`), plus
// `clipboard.image.offer` and `clipboard.image.accept`.

import Foundation
import WeftProtocol

public enum PacketConstants {
    public static let platformIOS = "ios"
    public static let maxFileSize: UInt64 = 4 * 1024 * 1024 * 1024
    public static let maxClipboardImageSize: UInt64 = 16 * 1024 * 1024
    public static let maxArtBytes = 128 * 1024
    public static let maxArtIDLength = 64
    public static let maxNotificationTextChars = 2000
    public static let maxCoversKept = 8

    public static let pngMagic: [UInt8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
    public static let jpegMagic: [UInt8] = [0xFF, 0xD8, 0xFF]

    public static func looksLikeImage(mime: String, data: Data) -> Bool {
        switch mime {
        case "image/png":
            return data.count >= pngMagic.count && data.prefix(pngMagic.count).elementsEqual(pngMagic)
        case "image/jpeg":
            return data.count >= jpegMagic.count && data.prefix(jpegMagic.count).elementsEqual(jpegMagic)
        default:
            return false
        }
    }

    public static func detectImageMime(data: Data) -> String? {
        if looksLikeImage(mime: "image/png", data: data) { return "image/png" }
        if looksLikeImage(mime: "image/jpeg", data: data) { return "image/jpeg" }
        return nil
    }

    public static func isValidArtID(_ id: String) -> Bool {
        guard (1...maxArtIDLength).contains(id.utf8.count) else { return false }
        return id.utf8.allSatisfy { byte in
            (0x30...0x39).contains(byte)
                || (0x41...0x5A).contains(byte)
                || (0x61...0x7A).contains(byte)
                || byte == 0x2D
                || byte == 0x5F
        }
    }

    /// Normalizes a 16-byte hash or transfer ID to 32 lowercase hex characters.
    public static func normalizeHex16(_ text: String) -> String? {
        let lower = text.lowercased()
        guard lower.utf8.count == 32,
              Data(hexString: lower, expectedCount: 16) != nil else {
            return nil
        }
        return lower
    }

    /// Accepts either 32-character hex or 24-character standard base64 of a 16-byte content hash,
    /// returning canonical 32-character lowercase hex.
    public static func normalizeHash16(_ text: String) -> String? {
        if let hex = normalizeHex16(text) {
            return hex
        }
        if let decoded = WeftProtocol.base64Decode(text), decoded.count == 16 {
            return decoded.hexString
        }
        return nil
    }
}

// MARK: - Payload structs

public struct CapabilitiesPayload: Codable, Equatable, Sendable {
    public var platform: String
    public var incoming: [String]
    public var outgoing: [String]

    public init(platform: String, incoming: [String], outgoing: [String]) {
        self.platform = platform
        self.incoming = incoming
        self.outgoing = outgoing
    }

    public func handles(_ packetType: String) -> Bool {
        incoming.contains(packetType)
    }

    public func sends(_ packetType: String) -> Bool {
        outgoing.contains(packetType)
    }
}

public struct BatteryPayload: Codable, Equatable, Sendable {
    public var level: UInt8?
    public var charging: Bool?
    public var low: Bool?

    public init(level: UInt8? = nil, charging: Bool? = nil, low: Bool? = nil) {
        self.level = level
        self.charging = charging
        self.low = low
    }
}

public struct ConnectivityPayload: Codable, Equatable, Sendable {
    public var network: String?
    public var signal: UInt8?

    public init(network: String? = nil, signal: UInt8? = nil) {
        self.network = network
        self.signal = signal
    }
}

public struct FileOfferPayload: Codable, Equatable, Sendable {
    public var id: String
    public var name: String
    public var size: UInt64
    public var mime: String?

    public init(id: String, name: String, size: UInt64, mime: String? = nil) {
        self.id = id.lowercased()
        self.name = name
        self.size = size
        self.mime = mime
    }
}

public struct ClipboardImageOfferPayload: Codable, Equatable, Sendable {
    public var id: String
    public var mime: String
    public var size: UInt64
    public var hash: String

    public init(id: String, mime: String, size: UInt64, hash: String) {
        self.id = id.lowercased()
        self.mime = mime
        self.size = size
        self.hash = hash.lowercased()
    }
}

public struct ClipboardImageAcceptPayload: Codable, Equatable, Sendable {
    public var id: String
    public var offset: UInt64?

    public init(id: String, offset: UInt64? = nil) {
        self.id = id.lowercased()
        self.offset = offset
    }
}

public struct MediaPlayerPayload: Codable, Equatable, Sendable, Identifiable {
    public var id: String
    public var app: String
    public var title: String
    public var artist: String
    public var album: String
    public var playing: Bool
    public var position: UInt64?
    public var length: UInt64?
    public var canPlay: Bool
    public var canPause: Bool
    public var canNext: Bool
    public var canPrevious: Bool
    public var canSeek: Bool
    public var volume: UInt8?
    public var art: String?

    public init(
        id: String,
        app: String = "",
        title: String = "",
        artist: String = "",
        album: String = "",
        playing: Bool = false,
        position: UInt64? = nil,
        length: UInt64? = nil,
        canPlay: Bool = false,
        canPause: Bool = false,
        canNext: Bool = false,
        canPrevious: Bool = false,
        canSeek: Bool = false,
        volume: UInt8? = nil,
        art: String? = nil
    ) {
        self.id = id
        self.app = app
        self.title = title
        self.artist = artist
        self.album = album
        self.playing = playing
        self.position = position
        self.length = length
        self.canPlay = canPlay
        self.canPause = canPause
        self.canNext = canNext
        self.canPrevious = canPrevious
        self.canSeek = canSeek
        self.volume = volume
        self.art = art
    }
}

public struct MediaArtPayload: Equatable, Sendable {
    public var art: String
    public var mime: String
    public var data: Data

    public init(art: String, mime: String, data: Data) {
        self.art = art
        self.mime = mime
        self.data = data
    }

    public var isValid: Bool {
        PacketConstants.isValidArtID(art)
            && data.count <= PacketConstants.maxArtBytes
            && PacketConstants.looksLikeImage(mime: mime, data: data)
    }
}

public enum MediaCommandAction: Equatable, Sendable {
    case play
    case pause
    case playPause
    case next
    case previous
    case seek(positionMs: UInt64)
    case setVolume(level: UInt8)

    public var wireName: String {
        switch self {
        case .play: "play"
        case .pause: "pause"
        case .playPause: "playPause"
        case .next: "next"
        case .previous: "previous"
        case .seek: "seek"
        case .setVolume: "setVolume"
        }
    }

    public var wireValue: UInt64? {
        switch self {
        case .seek(let ms): ms
        case .setVolume(let level): UInt64(level)
        case .play, .pause, .playPause, .next, .previous: nil
        }
    }
}

public struct MediaCommandPayload: Codable, Equatable, Sendable {
    public var player: String
    public var action: String
    public var value: UInt64?

    public init(player: String, action: String, value: UInt64? = nil) {
        self.player = player
        self.action = action
        self.value = value
    }

    public init(player: String, command: MediaCommandAction) {
        self.player = player
        self.action = command.wireName
        self.value = command.wireValue
    }

    public var parsedAction: MediaCommandAction? {
        switch action {
        case "play": return .play
        case "pause": return .pause
        case "playPause": return .playPause
        case "next": return .next
        case "previous": return .previous
        case "seek":
            guard let value else { return nil }
            return .seek(positionMs: value)
        case "setVolume":
            guard let value, value <= 100 else { return nil }
            return .setVolume(level: UInt8(value))
        default:
            return nil
        }
    }
}

public struct VolumePayload: Codable, Equatable, Sendable {
    public var level: UInt8?
    public var muted: Bool?

    public init(level: UInt8? = nil, muted: Bool? = nil) {
        self.level = level
        self.muted = muted
    }
}

public struct VolumeSetPayload: Codable, Equatable, Sendable {
    public var level: UInt8?
    public var muted: Bool?

    public init(level: UInt8? = nil, muted: Bool? = nil) {
        self.level = level
        self.muted = muted
    }
}

public struct InputPointerPayload: Codable, Equatable, Sendable {
    public var dx: Double?
    public var dy: Double?
    public var scrollX: Double?
    public var scrollY: Double?

    public init(dx: Double? = nil, dy: Double? = nil, scrollX: Double? = nil, scrollY: Double? = nil) {
        self.dx = dx
        self.dy = dy
        self.scrollX = scrollX
        self.scrollY = scrollY
    }
}

public struct InputClickPayload: Equatable, Sendable {
    public static let buttonLeft = "left"
    public static let buttonRight = "right"
    public static let buttonMiddle = "middle"

    public var button: String
    public var double: Bool
    public var down: Bool
    public var up: Bool

    public init(button: String, double: Bool = false, down: Bool = false, up: Bool = false) {
        self.button = button
        self.double = double
        self.down = down
        self.up = up
    }
}

public struct InputKeyPayload: Equatable, Sendable {
    public static let validNamedKeys: Set<String> = [
        "enter", "backspace", "tab", "escape",
        "left", "right", "up", "down",
        "home", "end", "pageUp", "pageDown", "delete",
    ]

    public var text: String?
    public var key: String?
    public var ctrl: Bool
    public var alt: Bool
    public var shift: Bool
    public var meta: Bool

    public init(
        text: String? = nil,
        key: String? = nil,
        ctrl: Bool = false,
        alt: Bool = false,
        shift: Bool = false,
        meta: Bool = false
    ) {
        self.text = text
        self.key = key
        self.ctrl = ctrl
        self.alt = alt
        self.shift = shift
        self.meta = meta
    }
}

public struct PresenterSpot: Codable, Equatable, Sendable {
    public var x: Double
    public var y: Double

    public init(x: Double, y: Double) {
        self.x = x
        self.y = y
    }
}

public enum PresenterPointer: Equatable, Sendable {
    /// `pointer` field omitted from JSON (e.g. slide navigation actions).
    case absent
    /// `"pointer": null` on the wire, hiding the laser highlight spot.
    case hidden
    /// `"pointer": {"x": ..., "y": ...}` on the wire (`-1...1`).
    case spot(PresenterSpot)
}

public struct PresenterPayload: Equatable, Sendable {
    public static let validActions: Set<String> = ["next", "previous", "start", "stop"]

    public var action: String?
    public var pointer: PresenterPointer

    public init(action: String? = nil, pointer: PresenterPointer = .absent) {
        self.action = action
        self.pointer = pointer
    }
}

public struct CommandItemPayload: Codable, Equatable, Sendable, Identifiable {
    public var id: String
    public var name: String

    public init(id: String, name: String) {
        self.id = id
        self.name = name
    }
}

// MARK: - Packet enum

public enum Packet: Equatable, Sendable {
    case capabilities(CapabilitiesPayload)
    case stateRequest
    case battery(BatteryPayload)
    case connectivity(ConnectivityPayload)
    case ring
    case ringStop
    case ping(message: String?)
    case shareURL(url: String)
    case fileOffer(FileOfferPayload)
    case fileAccept(id: String, offset: UInt64)
    case fileReject(id: String, reason: String?)
    case fileAck(id: String, offset: UInt64)
    case fileDone(id: String)
    case fileCancel(id: String)
    case clipboardImageOffer(ClipboardImageOfferPayload)
    case clipboardImageAccept(ClipboardImageAcceptPayload)
    case media(players: [MediaPlayerPayload])
    case mediaArt(MediaArtPayload)
    case mediaCommand(MediaCommandPayload)
    case volume(VolumePayload)
    case volumeSet(VolumeSetPayload)
    case inputPointer(InputPointerPayload)
    case inputClick(InputClickPayload)
    case inputKey(InputKeyPayload)
    case presenter(PresenterPayload)
    case commandList(commands: [CommandItemPayload])
    case commandRun(id: String)

    public static let knownTypes: Set<String> = [
        "capabilities",
        "state.request",
        "battery",
        "connectivity",
        "ring",
        "ring.stop",
        "ping",
        "share.url",
        "file.offer",
        "file.accept",
        "file.reject",
        "file.ack",
        "file.done",
        "file.cancel",
        "clipboard.image.offer",
        "clipboard.image.accept",
        "media",
        "media.art",
        "media.command",
        "volume",
        "volume.set",
        "input.pointer",
        "input.click",
        "input.key",
        "presenter",
        "command.list",
        "command.run",
    ]

    public var packetType: String {
        switch self {
        case .capabilities: "capabilities"
        case .stateRequest: "state.request"
        case .battery: "battery"
        case .connectivity: "connectivity"
        case .ring: "ring"
        case .ringStop: "ring.stop"
        case .ping: "ping"
        case .shareURL: "share.url"
        case .fileOffer: "file.offer"
        case .fileAccept: "file.accept"
        case .fileReject: "file.reject"
        case .fileAck: "file.ack"
        case .fileDone: "file.done"
        case .fileCancel: "file.cancel"
        case .clipboardImageOffer: "clipboard.image.offer"
        case .clipboardImageAccept: "clipboard.image.accept"
        case .media: "media"
        case .mediaArt: "media.art"
        case .mediaCommand: "media.command"
        case .volume: "volume"
        case .volumeSet: "volume.set"
        case .inputPointer: "input.pointer"
        case .inputClick: "input.click"
        case .inputKey: "input.key"
        case .presenter: "presenter"
        case .commandList: "command.list"
        case .commandRun: "command.run"
        }
    }
}

public enum ParsedPacket: Equatable, Sendable {
    case known(Packet)
    case unknown(type: String)
    case invalid
}

// MARK: - Wire JSON encoding & tolerant parsing

extension Packet: Codable {
    private enum Keys: String, CodingKey {
        case type
        case platform, incoming, outgoing
        case level, charging, low
        case network, signal
        case message
        case url
        case id, name, size, mime, offset, reason, hash
        case players
        case art, data
        case player, action, value
        case muted
        case dx, dy, scrollX, scrollY
        case button, double, down, up
        case text, key, ctrl, alt, shift, meta
        case pointer
        case commands
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(packetType, forKey: .type)

        switch self {
        case .capabilities(let cap):
            try c.encode(cap.platform, forKey: .platform)
            try c.encode(cap.incoming, forKey: .incoming)
            try c.encode(cap.outgoing, forKey: .outgoing)

        case .stateRequest, .ring, .ringStop:
            break

        case .battery(let b):
            try c.encodeIfPresent(b.level, forKey: .level)
            try c.encodeIfPresent(b.charging, forKey: .charging)
            try c.encodeIfPresent(b.low, forKey: .low)

        case .connectivity(let conn):
            try c.encodeIfPresent(conn.network, forKey: .network)
            try c.encodeIfPresent(conn.signal, forKey: .signal)

        case .ping(let message):
            try c.encodeIfPresent(message, forKey: .message)

        case .shareURL(let url):
            try c.encode(url, forKey: .url)

        case .fileOffer(let offer):
            try c.encode(offer.id.lowercased(), forKey: .id)
            try c.encode(offer.name, forKey: .name)
            try c.encode(offer.size, forKey: .size)
            try c.encodeIfPresent(offer.mime, forKey: .mime)

        case .fileAccept(let id, let offset):
            try c.encode(id.lowercased(), forKey: .id)
            try c.encode(offset, forKey: .offset)

        case .fileReject(let id, let reason):
            try c.encode(id.lowercased(), forKey: .id)
            try c.encodeIfPresent(reason, forKey: .reason)

        case .fileAck(let id, let offset):
            try c.encode(id.lowercased(), forKey: .id)
            try c.encode(offset, forKey: .offset)

        case .fileDone(let id):
            try c.encode(id.lowercased(), forKey: .id)

        case .fileCancel(let id):
            try c.encode(id.lowercased(), forKey: .id)

        case .clipboardImageOffer(let offer):
            try c.encode(offer.id.lowercased(), forKey: .id)
            try c.encode(offer.mime, forKey: .mime)
            try c.encode(offer.size, forKey: .size)
            try c.encode(offer.hash.lowercased(), forKey: .hash)

        case .clipboardImageAccept(let accept):
            try c.encode(accept.id.lowercased(), forKey: .id)
            try c.encodeIfPresent(accept.offset, forKey: .offset)

        case .media(let players):
            var list = c.nestedUnkeyedContainer(forKey: .players)
            for p in players {
                var pc = list.nestedContainer(keyedBy: PlayerKeys.self)
                try pc.encode(p.id, forKey: .id)
                if !p.app.isEmpty { try pc.encode(p.app, forKey: .app) }
                if !p.title.isEmpty { try pc.encode(p.title, forKey: .title) }
                if !p.artist.isEmpty { try pc.encode(p.artist, forKey: .artist) }
                if !p.album.isEmpty { try pc.encode(p.album, forKey: .album) }
                try pc.encode(p.playing, forKey: .playing)
                try pc.encodeIfPresent(p.position, forKey: .position)
                try pc.encodeIfPresent(p.length, forKey: .length)
                try pc.encode(p.canPlay, forKey: .canPlay)
                try pc.encode(p.canPause, forKey: .canPause)
                try pc.encode(p.canNext, forKey: .canNext)
                try pc.encode(p.canPrevious, forKey: .canPrevious)
                try pc.encode(p.canSeek, forKey: .canSeek)
                try pc.encodeIfPresent(p.volume, forKey: .volume)
                try pc.encodeIfPresent(p.art, forKey: .art)
            }

        case .mediaArt(let art):
            try c.encode(art.art, forKey: .art)
            try c.encode(art.mime, forKey: .mime)
            try c.encode(WeftProtocol.base64Encode(art.data), forKey: .data)

        case .mediaCommand(let cmd):
            try c.encode(cmd.player, forKey: .player)
            try c.encode(cmd.action, forKey: .action)
            try c.encodeIfPresent(cmd.value, forKey: .value)

        case .volume(let vol):
            try c.encodeIfPresent(vol.level, forKey: .level)
            try c.encodeIfPresent(vol.muted, forKey: .muted)

        case .volumeSet(let set):
            try c.encodeIfPresent(set.level, forKey: .level)
            try c.encodeIfPresent(set.muted, forKey: .muted)

        case .inputPointer(let ptr):
            try c.encodeIfPresent(ptr.dx, forKey: .dx)
            try c.encodeIfPresent(ptr.dy, forKey: .dy)
            try c.encodeIfPresent(ptr.scrollX, forKey: .scrollX)
            try c.encodeIfPresent(ptr.scrollY, forKey: .scrollY)

        case .inputClick(let click):
            try c.encode(click.button, forKey: .button)
            // Wire rule (docs/features.md §9): boolean flags omitted when false.
            if click.double { try c.encode(true, forKey: .double) }
            if click.down { try c.encode(true, forKey: .down) }
            if click.up { try c.encode(true, forKey: .up) }

        case .inputKey(let key):
            try c.encodeIfPresent(key.text, forKey: .text)
            try c.encodeIfPresent(key.key, forKey: .key)
            if key.ctrl { try c.encode(true, forKey: .ctrl) }
            if key.alt { try c.encode(true, forKey: .alt) }
            if key.shift { try c.encode(true, forKey: .shift) }
            if key.meta { try c.encode(true, forKey: .meta) }

        case .presenter(let pres):
            try c.encodeIfPresent(pres.action, forKey: .action)
            switch pres.pointer {
            case .absent:
                break
            case .hidden:
                try c.encodeNil(forKey: .pointer)
            case .spot(let spot):
                try c.encode(spot, forKey: .pointer)
            }

        case .commandList(let commands):
            try c.encode(commands, forKey: .commands)

        case .commandRun(let id):
            try c.encode(id, forKey: .id)
        }
    }

    public init(from decoder: any Decoder) throws {
        // Bridge through JSONSerialization via a generic JSONValue so unknown members
        // and tolerant array parsing match docs/features.md identically.
        let raw = try JSONValue(from: decoder)
        guard case .known(let packet) = Packet.parseValue(raw) else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(codingPath: decoder.codingPath, debugDescription: "Invalid or unknown Weft packet")
            )
        }
        self = packet
    }

    private enum PlayerKeys: String, CodingKey {
        case id, app, title, artist, album, playing, position, length
        case canPlay, canPause, canNext, canPrevious, canSeek, volume, art
    }

    public func encodeJSONData() throws -> Data {
        let encoder = JSONEncoder()
        return try encoder.encode(self)
    }

    public func encodeJSONString() throws -> String {
        let data = try encodeJSONData()
        return String(decoding: data, as: UTF8.self)
    }

    public static func parse(jsonData: Data) -> ParsedPacket {
        guard !jsonData.isEmpty, jsonData.count <= Weft.maxPacketLength else {
            return .invalid
        }
        guard let value = try? JSONDecoder().decode(JSONValue.self, from: jsonData) else {
            return .invalid
        }
        return parseValue(value)
    }

    public static func parse(jsonString: String) -> ParsedPacket {
        parse(jsonData: Data(jsonString.utf8))
    }

    fileprivate static func parseValue(_ root: JSONValue) -> ParsedPacket {
        guard case .object(let obj) = root,
              let typeVal = obj["type"],
              case .string(let type) = typeVal else {
            return .invalid
        }

        if !knownTypes.contains(type) {
            return .unknown(type: type)
        }

        switch type {
        case "capabilities":
            guard case .string(let platform)? = obj["platform"], !platform.isEmpty else {
                return .invalid
            }
            let incoming = extractStrings(obj["incoming"])
            let outgoing = extractStrings(obj["outgoing"])
            return .known(.capabilities(CapabilitiesPayload(
                platform: platform,
                incoming: incoming,
                outgoing: outgoing
            )))

        case "state.request":
            return .known(.stateRequest)

        case "battery":
            let level: UInt8?
            if let rawLevel = obj["level"], rawLevel != .null {
                guard let parsed = rawLevel.exactUInt64, parsed <= 100 else { return .invalid }
                level = UInt8(parsed)
            } else {
                level = nil
            }
            let charging: Bool?
            if let rawCharging = obj["charging"], rawCharging != .null {
                guard case .bool(let flag) = rawCharging else { return .invalid }
                charging = flag
            } else {
                charging = nil
            }
            let low: Bool?
            if let rawLow = obj["low"], rawLow != .null {
                guard case .bool(let flag) = rawLow else { return .invalid }
                low = flag
            } else {
                low = nil
            }
            return .known(.battery(BatteryPayload(level: level, charging: charging, low: low)))

        case "connectivity":
            let network: String?
            if let rawNet = obj["network"], rawNet != .null {
                guard case .string(let s) = rawNet else { return .invalid }
                network = s
            } else {
                network = nil
            }
            let signal: UInt8?
            if let rawSig = obj["signal"], rawSig != .null {
                // Out-of-range optional signal counts as unknown (nil), matching Android PacketsTest.
                if let val = rawSig.exactUInt64, val <= 4 {
                    signal = UInt8(val)
                } else if rawSig.isNumber {
                    signal = nil
                } else {
                    return .invalid
                }
            } else {
                signal = nil
            }
            return .known(.connectivity(ConnectivityPayload(network: network, signal: signal)))

        case "ring":
            return .known(.ring)

        case "ring.stop":
            return .known(.ringStop)

        case "ping":
            if let rawMsg = obj["message"], rawMsg != .null {
                guard case .string(let msg) = rawMsg else { return .invalid }
                return .known(.ping(message: msg))
            }
            return .known(.ping(message: nil))

        case "share.url":
            guard case .string(let url)? = obj["url"], Links.isShareable(url) else {
                return .invalid
            }
            return .known(.shareURL(url: url))

        case "file.offer":
            guard case .string(let rawID)? = obj["id"],
                  let id = PacketConstants.normalizeHex16(rawID),
                  case .string(let name)? = obj["name"],
                  !name.isEmpty,
                  let rawSize = obj["size"],
                  let size = rawSize.exactUInt64 else {
                return .invalid
            }
            let mime: String?
            if let rawMime = obj["mime"], rawMime != .null {
                guard case .string(let m) = rawMime else { return .invalid }
                mime = m
            } else {
                mime = nil
            }
            return .known(.fileOffer(FileOfferPayload(id: id, name: name, size: size, mime: mime)))

        case "file.accept":
            guard case .string(let rawID)? = obj["id"],
                  let id = PacketConstants.normalizeHex16(rawID),
                  let rawOffset = obj["offset"],
                  let offset = rawOffset.exactUInt64 else {
                return .invalid
            }
            return .known(.fileAccept(id: id, offset: offset))

        case "file.reject":
            guard case .string(let rawID)? = obj["id"],
                  let id = PacketConstants.normalizeHex16(rawID) else {
                return .invalid
            }
            let reason: String?
            if let rawReason = obj["reason"], rawReason != .null {
                guard case .string(let r) = rawReason else { return .invalid }
                reason = r
            } else {
                reason = nil
            }
            return .known(.fileReject(id: id, reason: reason))

        case "file.ack":
            guard case .string(let rawID)? = obj["id"],
                  let id = PacketConstants.normalizeHex16(rawID),
                  let rawOffset = obj["offset"],
                  let offset = rawOffset.exactUInt64 else {
                return .invalid
            }
            return .known(.fileAck(id: id, offset: offset))

        case "file.done":
            guard case .string(let rawID)? = obj["id"],
                  let id = PacketConstants.normalizeHex16(rawID) else {
                return .invalid
            }
            return .known(.fileDone(id: id))

        case "file.cancel":
            guard case .string(let rawID)? = obj["id"],
                  let id = PacketConstants.normalizeHex16(rawID) else {
                return .invalid
            }
            return .known(.fileCancel(id: id))

        case "clipboard.image.offer":
            guard case .string(let rawID)? = obj["id"],
                  let id = PacketConstants.normalizeHex16(rawID),
                  case .string(let mime)? = obj["mime"],
                  (mime == "image/png" || mime == "image/jpeg"),
                  let rawSize = obj["size"],
                  let size = rawSize.exactUInt64,
                  size >= 1 && size <= PacketConstants.maxClipboardImageSize,
                  case .string(let rawHash)? = obj["hash"],
                  let hash = PacketConstants.normalizeHash16(rawHash) else {
                return .invalid
            }
            return .known(.clipboardImageOffer(ClipboardImageOfferPayload(
                id: id,
                mime: mime,
                size: size,
                hash: hash
            )))

        case "clipboard.image.accept":
            guard case .string(let rawID)? = obj["id"],
                  let id = PacketConstants.normalizeHex16(rawID) else {
                return .invalid
            }
            let offset: UInt64?
            if let rawOffset = obj["offset"], rawOffset != .null {
                guard let parsed = rawOffset.exactUInt64 else { return .invalid }
                offset = parsed
            } else {
                offset = nil
            }
            return .known(.clipboardImageAccept(ClipboardImageAcceptPayload(id: id, offset: offset)))

        case "media":
            guard case .array(let rawPlayers)? = obj["players"] else {
                return .invalid
            }
            var players: [MediaPlayerPayload] = []
            for item in rawPlayers {
                guard case .object(let po) = item,
                      case .string(let pid)? = po["id"],
                      !pid.isEmpty else {
                    continue
                }
                let app = po["app"]?.stringValue ?? ""
                let title = po["title"]?.stringValue ?? ""
                let artist = po["artist"]?.stringValue ?? ""
                let album = po["album"]?.stringValue ?? ""
                let playing = po["playing"]?.boolValue ?? false
                let position = po["position"]?.exactUInt64
                let length = po["length"]?.exactUInt64
                let canPlay = po["canPlay"]?.boolValue ?? false
                let canPause = po["canPause"]?.boolValue ?? false
                let canNext = po["canNext"]?.boolValue ?? false
                let canPrevious = po["canPrevious"]?.boolValue ?? false
                let canSeek = po["canSeek"]?.boolValue ?? false
                let volume: UInt8? = if let v = po["volume"]?.exactUInt64, v <= 100 { UInt8(v) } else { nil }
                let art: String? = if let a = po["art"]?.stringValue, PacketConstants.isValidArtID(a) { a } else { nil }
                players.append(MediaPlayerPayload(
                    id: pid,
                    app: app,
                    title: title,
                    artist: artist,
                    album: album,
                    playing: playing,
                    position: position,
                    length: length,
                    canPlay: canPlay,
                    canPause: canPause,
                    canNext: canNext,
                    canPrevious: canPrevious,
                    canSeek: canSeek,
                    volume: volume,
                    art: art
                ))
            }
            return .known(.media(players: players))

        case "media.art":
            guard case .string(let artID)? = obj["art"],
                  case .string(let mime)? = obj["mime"],
                  case .string(let b64)? = obj["data"] else {
                return .invalid
            }
            let maxBase64Chars = ((PacketConstants.maxArtBytes + 2) / 3) * 4
            guard b64.utf8.count <= maxBase64Chars,
                  let decoded = WeftProtocol.base64Decode(b64) else {
                return .invalid
            }
            let payload = MediaArtPayload(art: artID, mime: mime, data: decoded)
            guard payload.isValid else { return .invalid }
            return .known(.mediaArt(payload))

        case "media.command":
            guard case .string(let player)? = obj["player"], !player.isEmpty,
                  case .string(let action)? = obj["action"] else {
                return .invalid
            }
            let value: UInt64?
            if let rawVal = obj["value"], rawVal != .null {
                guard let parsed = rawVal.exactUInt64 else { return .invalid }
                value = parsed
            } else {
                value = nil
            }
            let cmd = MediaCommandPayload(player: player, action: action, value: value)
            guard cmd.parsedAction != nil else { return .invalid }
            return .known(.mediaCommand(cmd))

        case "volume":
            let level: UInt8?
            if let rawLevel = obj["level"], rawLevel != .null {
                guard let parsed = rawLevel.exactUInt64, parsed <= 100 else { return .invalid }
                level = UInt8(parsed)
            } else {
                level = nil
            }
            let muted: Bool?
            if let rawMuted = obj["muted"], rawMuted != .null {
                guard case .bool(let flag) = rawMuted else { return .invalid }
                muted = flag
            } else {
                muted = nil
            }
            return .known(.volume(VolumePayload(level: level, muted: muted)))

        case "volume.set":
            let level: UInt8?
            if let rawLevel = obj["level"], rawLevel != .null {
                if let parsed = rawLevel.exactUInt64, parsed <= 100 {
                    level = UInt8(parsed)
                } else if rawLevel.isNumber {
                    level = nil
                } else {
                    return .invalid
                }
            } else {
                level = nil
            }
            let muted: Bool?
            if let rawMuted = obj["muted"], rawMuted != .null {
                guard case .bool(let flag) = rawMuted else { return .invalid }
                muted = flag
            } else {
                muted = nil
            }
            return .known(.volumeSet(VolumeSetPayload(level: level, muted: muted)))

        case "input.pointer":
            guard let dx = tryOptionalDouble(obj["dx"]),
                  let dy = tryOptionalDouble(obj["dy"]),
                  let scrollX = tryOptionalDouble(obj["scrollX"]),
                  let scrollY = tryOptionalDouble(obj["scrollY"]) else {
                return .invalid
            }
            return .known(.inputPointer(InputPointerPayload(dx: dx, dy: dy, scrollX: scrollX, scrollY: scrollY)))

        case "input.click":
            guard case .string(let button)? = obj["button"],
                  (button == "left" || button == "right" || button == "middle"),
                  let double = tryOptionalBoolDefaultFalse(obj["double"]),
                  let down = tryOptionalBoolDefaultFalse(obj["down"]),
                  let up = tryOptionalBoolDefaultFalse(obj["up"]) else {
                return .invalid
            }
            return .known(.inputClick(InputClickPayload(button: button, double: double, down: down, up: up)))

        case "input.key":
            let text: String?
            if let rawText = obj["text"], rawText != .null {
                guard case .string(let s) = rawText else { return .invalid }
                text = s
            } else {
                text = nil
            }
            let key: String?
            if let rawKey = obj["key"], rawKey != .null {
                guard case .string(let k) = rawKey else { return .invalid }
                key = k
            } else {
                key = nil
            }
            guard let ctrl = tryOptionalBoolDefaultFalse(obj["ctrl"]),
                  let alt = tryOptionalBoolDefaultFalse(obj["alt"]),
                  let shift = tryOptionalBoolDefaultFalse(obj["shift"]),
                  let meta = tryOptionalBoolDefaultFalse(obj["meta"]) else {
                return .invalid
            }
            return .known(.inputKey(InputKeyPayload(
                text: text, key: key, ctrl: ctrl, alt: alt, shift: shift, meta: meta
            )))

        case "presenter":
            let action: String?
            if let rawAction = obj["action"], rawAction != .null {
                guard case .string(let act) = rawAction,
                      PresenterPayload.validActions.contains(act) else {
                    return .invalid
                }
                action = act
            } else {
                action = nil
            }
            let pointer: PresenterPointer
            if let rawPointer = obj["pointer"] {
                switch rawPointer {
                case .null:
                    pointer = .hidden
                case .object(let po):
                    guard let xOpt = tryOptionalDouble(po["x"]), let x = xOpt,
                          let yOpt = tryOptionalDouble(po["y"]), let y = yOpt else {
                        return .invalid
                    }
                    pointer = .spot(PresenterSpot(x: x, y: y))
                default:
                    return .invalid
                }
            } else {
                pointer = .absent
            }
            return .known(.presenter(PresenterPayload(action: action, pointer: pointer)))

        case "command.list":
            let commands: [CommandItemPayload]
            if let rawCmds = obj["commands"] {
                guard case .array(let arr) = rawCmds else { return .invalid }
                var parsed: [CommandItemPayload] = []
                for item in arr {
                    guard case .object(let co) = item,
                          case .string(let cid)? = co["id"],
                          case .string(let cname)? = co["name"] else {
                        return .invalid
                    }
                    parsed.append(CommandItemPayload(id: cid, name: cname))
                }
                commands = parsed
            } else {
                commands = []
            }
            return .known(.commandList(commands: commands))

        case "command.run":
            guard case .string(let id)? = obj["id"], !id.isEmpty else {
                return .invalid
            }
            return .known(.commandRun(id: id))

        default:
            return .unknown(type: type)
        }
    }

    private static func extractStrings(_ val: JSONValue?) -> [String] {
        guard case .array(let items)? = val else { return [] }
        return items.compactMap { item in
            if case .string(let s) = item { return s }
            return nil
        }
    }

    private static func tryOptionalDouble(_ val: JSONValue?) -> Double?? {
        guard let val, val != .null else { return .some(nil) }
        guard let d = val.doubleValue, d.isFinite else { return nil }
        return .some(d)
    }

    private static func tryOptionalBoolDefaultFalse(_ val: JSONValue?) -> Bool? {
        guard let val, val != .null else { return false }
        guard case .bool(let b) = val else { return nil }
        return b
    }
}

// MARK: - Lightweight Sendable JSON AST for exact wire validation

private enum JSONValue: Decodable, Equatable, Sendable {
    case null
    case bool(Bool)
    case int(Int64)
    case uint(UInt64)
    case double(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let b = try? container.decode(Bool.self) {
            self = .bool(b)
        } else if let u = try? container.decode(UInt64.self) {
            self = .uint(u)
        } else if let i = try? container.decode(Int64.self) {
            self = .int(i)
        } else if let d = try? container.decode(Double.self) {
            self = .double(d)
        } else if let s = try? container.decode(String.self) {
            self = .string(s)
        } else if let arr = try? container.decode([JSONValue].self) {
            self = .array(arr)
        } else if let obj = try? container.decode([String: JSONValue].self) {
            self = .object(obj)
        } else {
            throw DecodingError.dataCorruptedError(in: container, debugDescription: "Unsupported JSON token")
        }
    }

    var stringValue: String? {
        if case .string(let s) = self { return s }
        return nil
    }

    var boolValue: Bool? {
        if case .bool(let b) = self { return b }
        return nil
    }

    var isNumber: Bool {
        switch self {
        case .int, .uint, .double: true
        default: false
        }
    }

    var exactUInt64: UInt64? {
        switch self {
        case .uint(let u):
            return u
        case .int(let i) where i >= 0:
            return UInt64(i)
        case .double(let d) where d >= 0 && d.rounded(.towardZero) == d && d < Double(UInt64.max):
            return UInt64(d)
        default:
            return nil
        }
    }

    var doubleValue: Double? {
        switch self {
        case .double(let d): d
        case .uint(let u): Double(u)
        case .int(let i): Double(i)
        default: nil
        }
    }
}
