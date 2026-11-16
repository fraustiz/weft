package fr.frohub.weft.features

import android.view.KeyEvent

/** The Android key code of an `input.key` [key] (one of [Packet.KEYS]); null for anything else. */
fun keyCodeOf(key: String): Int? = when (key) {
    Packet.KEY_ENTER -> KeyEvent.KEYCODE_ENTER
    Packet.KEY_BACKSPACE -> KeyEvent.KEYCODE_DEL
    Packet.KEY_TAB -> KeyEvent.KEYCODE_TAB
    Packet.KEY_ESCAPE -> KeyEvent.KEYCODE_ESCAPE
    Packet.KEY_LEFT -> KeyEvent.KEYCODE_DPAD_LEFT
    Packet.KEY_RIGHT -> KeyEvent.KEYCODE_DPAD_RIGHT
    Packet.KEY_UP -> KeyEvent.KEYCODE_DPAD_UP
    Packet.KEY_DOWN -> KeyEvent.KEYCODE_DPAD_DOWN
    Packet.KEY_HOME -> KeyEvent.KEYCODE_MOVE_HOME
    Packet.KEY_END -> KeyEvent.KEYCODE_MOVE_END
    Packet.KEY_PAGE_UP -> KeyEvent.KEYCODE_PAGE_UP
    Packet.KEY_PAGE_DOWN -> KeyEvent.KEYCODE_PAGE_DOWN
    Packet.KEY_DELETE -> KeyEvent.KEYCODE_FORWARD_DEL
    else -> null
}

/** The key code of a letter or digit, for shortcuts like Ctrl+A; null for any other text. */
fun keyCodeOfChar(text: String): Int? {
    if (text.length != 1) return null
    val c = text[0].lowercaseChar()
    return when (c) {
        in 'a'..'z' -> KeyEvent.KEYCODE_A + (c - 'a')
        in '0'..'9' -> KeyEvent.KEYCODE_0 + (c - '0')
        else -> null
    }
}

/** The meta state for the modifiers of [packet]; 0 when none is held. */
fun metaStateOf(packet: Packet.InputKey): Int {
    var meta = 0
    if (packet.ctrl) meta = meta or KeyEvent.META_CTRL_ON or KeyEvent.META_CTRL_LEFT_ON
    if (packet.alt) meta = meta or KeyEvent.META_ALT_ON or KeyEvent.META_ALT_LEFT_ON
    if (packet.shift) meta = meta or KeyEvent.META_SHIFT_ON or KeyEvent.META_SHIFT_LEFT_ON
    if (packet.meta) meta = meta or KeyEvent.META_META_ON or KeyEvent.META_META_LEFT_ON
    return meta
}
