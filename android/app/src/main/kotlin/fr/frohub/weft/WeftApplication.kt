package fr.frohub.weft

import android.app.Application
import android.content.Context
import fr.frohub.weft.core.WeftCore
import kotlinx.coroutines.launch

class WeftApplication : Application() {
    /** Identity, state, relay connection and pairing, created on first use. */
    val core: WeftCore by lazy { WeftCore(this) }

    private var senderOverride: ClipboardSender? = null

    /** Where every entry point sends text: the relay. Tests swap it for a fake. */
    var clipboardSender: ClipboardSender
        get() = senderOverride ?: core.sender
        set(value) {
            senderOverride = value.takeUnless { it === core.sender }
        }

    /** Starts [WeftService] if this device is paired. Call it from a visible activity. */
    fun startServiceIfPaired() {
        core.scope.launch {
            if (core.isPaired()) {
                try {
                    WeftService.start(this@WeftApplication)
                } catch (e: IllegalStateException) {
                    android.util.Log.w("Weft", "could not start the service: ${e.javaClass.simpleName}")
                }
            }
        }
    }
}

val Context.weft: WeftApplication
    get() = applicationContext as WeftApplication
