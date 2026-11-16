package fr.frohub.weft

import android.app.Application
import android.content.Context
import fr.frohub.weft.core.WeftCore
import fr.frohub.weft.features.Features
import fr.frohub.weft.ui.CoreModel
import kotlinx.coroutines.launch

class WeftApplication : Application() {
    /** Identity, state, relay connection and pairing, created on first use. */
    val core: WeftCore by lazy { WeftCore(this) }

    /**
     * Packets beyond the clipboard (docs/features.md). Every entry point that connects (the
     * screens, the service, the share targets) creates it first, so no packet goes unheard.
     */
    val features: Features by lazy { Features(this, core) }

    /** What the screens use. */
    val model: CoreModel by lazy { CoreModel(core, features) }

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
