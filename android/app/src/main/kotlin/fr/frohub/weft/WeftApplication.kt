package fr.frohub.weft

import android.app.Application
import android.content.Context

class WeftApplication : Application() {
    val memorySender = InMemoryClipboardSender()

    /** Where every entry point sends text. Tests swap it for a fake. */
    var clipboardSender: ClipboardSender = memorySender
}

val Context.weft: WeftApplication
    get() = applicationContext as WeftApplication
