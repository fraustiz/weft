package fr.frohub.weft

import android.app.Activity
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.widget.Toast
import androidx.annotation.StringRes
import androidx.core.content.IntentCompat
import fr.frohub.weft.features.isShareableUrl
import fr.frohub.weft.features.readClipboardImage
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Invisible activity that sends the clipboard, like KDE Connect's ClipboardFloatingActivity.
 *
 * Since Android 10 an app can read the clipboard only while it has window focus, so the read
 * happens in [onWindowFocusChanged], then the activity finishes at once.
 *
 * It also receives text shared with Weft (through the exported `.ShareTextActivity` alias); that
 * path sends the shared text and never touches the clipboard, so other apps cannot use the alias
 * to read it. A shared text that is a single web link goes as a link instead.
 */
class ClipboardSendActivity : Activity() {
    private var done = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // From the tile after a cold start, the connection is not up yet: bring it up now,
        // while this activity makes the app foreground; the send waits for it.
        weft.startServiceIfPaired()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            overrideActivityTransition(OVERRIDE_TRANSITION_OPEN, 0, 0)
            overrideActivityTransition(OVERRIDE_TRANSITION_CLOSE, 0, 0)
        }
        val launchedDirectly = intent.component?.className == ClipboardSendActivity::class.java.name
        if (!launchedDirectly || intent.action == Intent.ACTION_SEND) {
            done = true
            if (intent.action == Intent.ACTION_SEND && intent.type in setOf("image/png", "image/jpeg")) {
                sendSharedImage()
                return
            }
            if (intent.action == Intent.ACTION_SEND) sendShared()
            finish()
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (!hasFocus || done) return
        done = true
        sendClipboard()
        finish()
    }

    @Suppress("DEPRECATION")
    override fun finish() {
        super.finish()
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE) overridePendingTransition(0, 0)
    }

    private fun sendShared() {
        val text = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()
        if (text.isNullOrEmpty()) return toast(R.string.toast_no_text)
        // A single web link opens on the other side (docs/features.md, section 4).
        val link = text.trim()
        if (isShareableUrl(link)) sendLink(link) else send(text, R.string.toast_text_sent)
    }

    private fun sendSharedImage() {
        if (intent.clipData?.description?.extras?.getBoolean(EXTRA_IS_SENSITIVE) == true) {
            toast(R.string.toast_sensitive)
            return finish()
        }
        val uri = IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, android.net.Uri::class.java)
        if (uri == null) { toast(R.string.toast_clipboard_not_text); return finish() }
        val app = weft
        app.core.scope.launch {
            val image = readClipboardImage(app, uri)
            val result = image?.let { app.features.sendClipboardImage(it.first, it.second) }
            val message = result?.let { toastFor(it, R.string.toast_image_requested) } ?: R.string.toast_clipboard_not_text
            withContext(Dispatchers.Main) {
                Toast.makeText(app, message, Toast.LENGTH_SHORT).show()
                finish()
            }
        }
    }

    private fun sendLink(url: String) {
        val app = weft
        app.core.scope.launch {
            val message = toastFor(app.features.shareUrl(url), R.string.toast_link_sent)
            withContext(Dispatchers.Main) { Toast.makeText(app, message, Toast.LENGTH_SHORT).show() }
        }
    }

    private fun sendClipboard() {
        val clip = getSystemService(ClipboardManager::class.java).primaryClip
        if (clip == null || clip.itemCount == 0) return toast(R.string.toast_clipboard_empty)
        // ClipDescription.EXTRA_IS_SENSITIVE is public from API 33, but password managers set the
        // same key on older versions too, so it is honoured everywhere.
        if (clip.description.extras?.getBoolean(EXTRA_IS_SENSITIVE) == true) {
            return toast(R.string.toast_sensitive)
        }
        val item = clip.getItemAt(0)
        if (item.text == null && item.uri != null) {
            val app = weft
            val uri = item.uri
            app.core.scope.launch {
                val image = readClipboardImage(app, uri)
                val result = image?.let { app.features.sendClipboardImage(it.first, it.second) }
                val message = result?.let { toastFor(it, R.string.toast_image_requested) } ?: R.string.toast_clipboard_not_text
                withContext(Dispatchers.Main) { Toast.makeText(app, message, Toast.LENGTH_SHORT).show() }
            }
            return
        }
        val text = item.text?.toString() ?: return toast(R.string.toast_clipboard_not_text)
        if (text.isEmpty()) return toast(R.string.toast_clipboard_empty)
        send(text, R.string.toast_clipboard_sent)
    }

    /**
     * Sends in the application scope: this activity finishes at once and must not hold the
     * screen while the relay connects. The toast comes when the send is done.
     */
    private fun send(text: String, @StringRes sentMessage: Int) {
        val app = weft
        val sender = app.clipboardSender
        app.core.scope.launch {
            val message = toastFor(sender.send(text), sentMessage)
            withContext(Dispatchers.Main) { Toast.makeText(app, message, Toast.LENGTH_SHORT).show() }
        }
    }

    private fun toast(@StringRes message: Int) {
        Toast.makeText(applicationContext, message, Toast.LENGTH_SHORT).show()
    }

    companion object {
        private const val EXTRA_IS_SENSITIVE = "android.content.extra.IS_SENSITIVE"

        /** Reads the clipboard and sends it. Also used as the tile's target. */
        fun intent(context: Context): Intent =
            Intent(context, ClipboardSendActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION)
    }
}

/** The toast that tells how a send ended. */
@StringRes
private fun toastFor(result: SendResult, @StringRes sentMessage: Int): Int = when (result) {
    is SendResult.Sent -> sentMessage
    SendResult.NotConnected -> R.string.toast_not_connected
    SendResult.NoPairedDevice -> R.string.toast_no_paired_device
    SendResult.TooLarge -> R.string.toast_too_large
}
