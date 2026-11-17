# Weft features — packets (draft)

This document extends [`protocol.md`](protocol.md) with the features KDE Connect has beyond the clipboard: files and links, notifications, battery, ring, calls, media, volume, SMS, contacts, remote input, presenter and commands. It is the contract between the Android and Windows apps (and later macOS).

Out of scope: remote file system (KDE Connect `sftp`) and graphics tablet (`digitizer`).

## 1. Two new envelope kinds

| Value | Meaning | Plaintext |
| --- | --- | --- |
| `0x07` | Packet | a UTF-8 JSON object, 1 byte to 256 KiB (same cap as clipboard text) |
| `0x08` | File chunk | `transfer_id (16) \|\| offset (u64, big-endian) \|\| data (1 to 131072 bytes)` |

Both use the envelope, keys, counters and replay protection of `protocol.md` unchanged.

- **Packet.** The object has a string member `type` (for example `"battery"`) and the members listed for that type below. Clients **ignore** a packet whose `type` they do not know, and ignore members they do not know. A packet that is not a JSON object, or has no string `type`, is dropped. Member names are camelCase. A missing optional member means "unknown" or "not supported", never `0` or `false`.
- **File chunk.** Binary, so a file costs no base64 overhead. `transfer_id` ties it to a `file.offer` (section 4).
- **Relay.** From this revision on, the relay routes an envelope on its header **without checking `kind`**: it only checks `version`, the length, the sender and the group. Features added later need no relay update. The relay still caps a frame at `58 + 24 + 256 KiB + 16` bytes.

## 2. Capabilities

Whenever a device sends its Device name (kind `0x06`, `protocol.md` section 5), it also sends a `capabilities` packet to the same device. The other way round too: each time a device sends a peer its capabilities and state on a connection (after its own reconnection, a `state.request`, or a first `capabilities` from that peer), it sends its Device name right before, so a device that was offline when the name changed learns it on its next connection (the relay keeps nothing for offline devices). It may send its name with other `capabilities` packets too. The packet looks like this:

```json
{"type": "capabilities", "platform": "android", "incoming": ["ring", "file.offer", ...], "outgoing": ["battery", "notification", ...]}
```

- `platform`: `"android"`, `"windows"`, `"macos"` or `"cli"`.
- `incoming`: packet types this device acts on; `outgoing`: packet types it may send. A feature turned off in the local settings is left out of both lists, and a new `capabilities` packet is sent to every trusted device when the settings change.
- The receiver stores the last one per device and shows only the actions the other side handles (no "Ring" button for a device without `ring` in its `incoming`).
- When a device receives `capabilities` from a device it has no state for yet, it answers with its own (so both sides converge after either one restarts). A device also sends its current state packets (`battery`, `media`, `volume`…) right after sending its capabilities, so a fresh connection shows real values at once.

`state.request` `{}` asks the receiver to send again its capabilities and current state packets. A device sends it to each trusted device after it (re)connects to the relay.

## 3. Status: battery, connectivity, ring, ping

| Type | Direction | Members |
| --- | --- | --- |
| `battery` | phone → others, on change (at most once per minute unless `charging` or `low` changes) | `level` (0–100), `charging` (bool), `low` (bool, the system's low-battery state) |
| `connectivity` | phone → others, on change | `network` (`"5G"`, `"LTE"`, `"3G"`, `"2G"`, `"wifi"`, `"none"`, or absent), `signal` (0–4, absent if unknown) |
| `ring` | any → any | — The receiver plays a loud sound (alarm stream on Android, even in silent mode) and shows a full-screen or top-level notice with a "Found it" button, until the button is pressed or 60 s pass. |
| `ring.stop` | any → any | — Stops a ring this device started. |
| `ping` | any → any | `message` (optional string). Shows a plain notification "Ping from <name>". |

## 4. Files and links

### Links
`share.url` `{"url": "https://…"}` — only `http` and `https` URLs, 2048 bytes at most. Windows opens it in the default browser. Android shows a notification "Link from <name>" that opens it (Android forbids starting an activity from the background).

### Files
A transfer is identified by a random 16-byte `transfer_id`, written as 32 lowercase hex characters in packets (`id`).

1. The sender sends `file.offer` `{"id", "name", "size", "mime"}`. `name` is the file name only (no path), `size` in bytes (0 to 4 GiB), `mime` optional.
2. The receiver answers `file.accept` `{"id", "offset"}` (offset = bytes it already has, 0 for a new transfer) or `file.reject` `{"id", "reason"}` (`"disabled"`, `"tooLarge"`, `"noSpace"`, `"busy"`). Files from a trusted device are accepted automatically when the feature is on.
3. The sender sends kind `0x08` chunks of 128 KiB (the last one shorter) in order, from `offset`. It keeps **at most 8 chunks without acknowledgement** in flight.
4. The receiver sends `file.ack` `{"id", "offset"}` after each chunk it has written (offset = total bytes written). A chunk whose offset is not exactly the next expected byte is ignored.
5. When the receiver has `size` bytes it closes the file, sends `file.done` `{"id"}` and shows "Received <name>". A zero-byte file is done at once after `file.accept`.
6. Either side may send `file.cancel` `{"id"}`; the receiver then deletes the partial file.

Resume: if the connection drops, the receiver keeps the partial file (in a private folder, not in Downloads) for 24 hours. When the sender is back online it sends the same `file.offer` again, and the receiver answers `file.accept` with the offset it has. A sender gives up after 10 minutes without progress and tells its user.

Receiver rules: the name is sanitised (no path separators, no `..`, no reserved Windows names, no control characters, at most 200 bytes, the extension kept); an existing file is never overwritten (`name (1).ext`, `name (2).ext`…); the file is written to a private temporary place and moved to Downloads only when complete; nothing received is ever opened or run automatically. Several transfers may run at once, at most 4 per direction and per device pair; extra offers are rejected with `"busy"`.

Delivery to an offline device (stored on the relay) is a later revision.

## 5. Notifications and calls

| Type | Direction | Members |
| --- | --- | --- |
| `notification` | phone → others | `id` (string, stable for one notification), `app` (app label), `appId` (package name), `title`, `text`, `time` (ms since epoch), `dismissable` (bool), `actions` (array of action labels, optional), `reply` (bool: the notification has an inline reply), `silent` (bool: an update that should not pop up again) |
| `notification.removed` | phone → others | `id` |
| `notification.dismiss` | computer → phone | `id` — dismisses it on the phone |
| `notification.action` | computer → phone | `id`, `action` (one of its labels) |
| `notification.reply` | computer → phone | `id`, `text` |
| `notification.request` | computer → phone | — asks for every current notification again |
| `call` | phone → others | `event` (`"ringing"`, `"talking"`, `"missed"`, `"ended"`), `number` (optional), `name` (contact name, optional) |
| `call.mute` | computer → phone | — silences the ringer of the current incoming call |

Phone rules: never sent for Weft's own notifications, for ongoing or group-summary notifications, or for apps the user excluded. Android 15 hides one-time codes from listeners; Weft does not work around it. `text` is at most 2000 characters. Icons are not sent in this revision.

Computer rules: shown as system notifications (toasts) titled "<app> · <device>", with the actions and an inline reply box when offered, and a "Dismiss on phone" action when `dismissable`. A `notification.removed` removes the toast. A `call` with `ringing` shows a toast with "Mute"; `missed` shows a toast that stays in the notification centre.

## 6. Media and volume

| Type | Direction | Members |
| --- | --- | --- |
| `media` | any → others, on change | `players`: array of `{"id", "app", "title", "artist", "album", "playing" (bool), "position" (ms), "length" (ms), "canPlay", "canPause", "canNext", "canPrevious", "canSeek" (bools), "volume" (0–100, optional), "art" (optional string)}`. An empty array means nothing plays. `position` is the position at the moment of sending; the receiver extrapolates while `playing`. |
| `media.art` | any → others | `art` (the id a player's `art` names), `mime` (`"image/jpeg"` or `"image/png"`), `data` (standard base64, at most 128 KiB once decoded) |
| `media.command` | any → any | `player` (id), `action` (`"play"`, `"pause"`, `"playPause"`, `"next"`, `"previous"`, `"seek"`, `"setVolume"`), `value` (ms for `seek`, 0–100 for `setVolume`) |
| `volume` | any → others, on change | `level` (0–100), `muted` (bool) — the computer's default output device, the phone's media volume |
| `volume.set` | any → any | `level` (optional), `muted` (optional) |

Artwork: a player's `art` is an id its sender chooses for the current cover (for example a hash of the image), 1 to 64 ASCII letters, digits, `-` or `_`; the same image keeps the same id. The sender sends `media.art` for an id **before** the first `media` packet that names it on a connection, and again after its capabilities on a fresh connection; it never resends an id the peer already got on this connection. A connection, here, starts each time a device sends a peer its capabilities followed by its state (section 2: after its own reconnection, a `state.request`, or a first `capabilities` from that peer). A peer that does not list `media.art` in its `incoming` gets players without `art`. It scales the cover so that its longer side is at most 512 px (300 px is enough), JPEG unless the image needs transparency. The receiver keeps the last 8 covers per device, by id, in memory, and shows a player's cover only when it has the image of its `art`; a `media.art` it cannot decode, over the limit or with another `mime` is dropped. `media.art` belongs to the `media` feature: a device lists it in `incoming` only if it shows covers.

Phone: reads media sessions through the notification listener permission (`MediaSessionManager.getActiveSessions`). Computer: Windows global system media transport controls (the session thumbnail as cover), default render endpoint volume (Core Audio); macOS reads now-playing through [mediaremote-adapter](https://github.com/ungive/mediaremote-adapter) (BSD-3-Clause).

## 7. SMS and contacts

| Type | Direction | Members |
| --- | --- | --- |
| `sms.conversations.request` | computer → phone | — |
| `sms.conversations` | phone → computer | `conversations`: array of `{"thread", "addresses" (array of strings), "names" (array, same order, null when unknown), "snippet", "time", "unread" (bool)}`, newest first, at most 100 |
| `sms.messages.request` | computer → phone | `thread`, `before` (time, optional), `limit` (default 50) |
| `sms.messages` | phone → computer | `thread`, `messages`: array of `{"id", "address", "body", "time", "outgoing" (bool), "status" (`"sent"`, `"pending"`, `"failed"`, optional)}`, oldest first |
| `sms.send` | computer → phone | `addresses` (array), `text`, `requestId` |
| `sms.sent` | phone → computer | `requestId`, `ok` (bool), `error` (optional) |
| `sms.received` | phone → computer | `thread`, plus the members of one message |
| `contacts.request` | computer → phone | — |
| `contacts` | phone → computer | `contacts`: array of `{"name", "numbers" (array)}`, at most 5000 (split across packets of at most 200 KiB, each with `part` and `parts`) |

Phone rules: each needs its Android permission (`READ_SMS`, `SEND_SMS`, `RECEIVE_SMS`, `READ_CONTACTS`), asked from the app's settings, and the feature setting "Let computers read and send SMS" (off by default). MMS bodies are sent as their text part only in this revision. A packet over 256 KiB is split by sending fewer items.

## 8. Remote input, presenter, commands

| Type | Direction | Members |
| --- | --- | --- |
| `input.pointer` | phone → computer | `dx`, `dy` (pixels, may be fractional), or `scrollX`, `scrollY` (wheel notches, fractional) |
| `input.click` | phone → computer | `button` (`"left"`, `"right"`, `"middle"`), `double` (bool), or `down` / `up` (bool, for drag) |
| `input.key` | any → any | `text` (characters to type), or `key` (`"enter"`, `"backspace"`, `"tab"`, `"escape"`, `"left"`, `"right"`, `"up"`, `"down"`, `"home"`, `"end"`, `"pageUp"`, `"pageDown"`, `"delete"`), with optional `ctrl`, `alt`, `shift`, `meta` (bools) |
| `presenter` | phone → computer | `action` (`"next"`, `"previous"`, `"start"`, `"stop"`), or `pointer` `{"x", "y"}` (−1 to 1, from the phone's orientation) to move a highlight spot, `pointer: null` to hide it |
| `command.list` | computer → others | `commands`: array of `{"id", "name"}` |
| `command.run` | any → computer | `id` |

Safety rules:
- The computer acts on `input.*` and `presenter` only while the user has turned on "Let <device> control this computer" for that device (off by default, one switch per device, shown on the device's page whenever it sends `input.pointer`). It lists `input.pointer`, `input.click`, `input.key` and `presenter` in its `incoming` only in the `capabilities` it sends to a device allowed to control it, and sends that device fresh `capabilities` as soon as the switch changes, so the phone shows or hides its remote control at once. Typing on the phone (`input.key` in `outgoing`) does not depend on it.
- The phone types received `input.key` only through the Weft keyboard (an input method the user must pick in Android's settings, as KDE Connect does); otherwise it ignores it.
- Commands are defined **only on the computer** (name + command line, in its settings). `command.run` runs a listed command by `id`; nothing a peer sends is ever part of a command line.

## 9. Wire details

- SMS `thread` and message `id` are JSON integers; `requestId` and command `id` are strings.
- Every `time` is in milliseconds since the Unix epoch.
- Contacts `part` counts from 1 to `parts`.
- `scrollY > 0` means the wheel turned up (away from the user), `scrollX > 0` means scroll right.
- Presenter `pointer`: `x` from −1 (left) to 1 (right), `y` from −1 (top) to 1 (bottom).
- Boolean flags (`double`, `down`, `up`, `ctrl`, `alt`, `shift`, `meta`, `unread`, `outgoing`) are sent only when true; a missing flag is false.
- `sms.sent` `error`: `"permission"`, `"noSms"`, `"failed"`, `"noService"`, `"radioOff"`.

## 10. Local settings

Each app has one switch per feature (files, links, notifications, battery, ring, calls, media, volume, SMS, contacts, input, commands), on by default except SMS, contacts and input. A switched-off feature is removed from `capabilities`; packets for it are ignored. `capabilities` may differ per recipient: a computer has no global input switch, and lists `input.*` and `presenter` as `incoming` only to the devices allowed to control it (section 8).

On the phone, "input" means only being typed into (the Weft keyboard, off by default); sending remote control and presenter packets is a separate "remote control" switch, on by default, since the computer accepts them only after its user allowed that phone.

The phone also has a "media controls notification" switch under media, on by default: each connected computer that reports a player (the playing one, else the first) gets a media notification with its own media session (title, artist, cover, play/pause, previous, next, seek when `canSeek`), whose buttons send `media.command`. It goes away when the computer reports no players, is removed, the relay connection drops or the switch is off. The phone never reports these sessions back as its own players. It changes nothing in `capabilities`.
