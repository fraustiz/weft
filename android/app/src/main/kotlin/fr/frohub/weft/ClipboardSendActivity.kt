package fr.frohub.weft

import android.app.Activity
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.widget.Toast
import androidx.annotation.StringRes

/**
 * Invisible activity that sends the clipboard, like KDE Connect's ClipboardFloatingActivity.
 *
 * Since Android 10 an app can read the clipboard only while it has window focus, so the read
 * happens in [onWindowFocusChanged], then the activity finishes at once.
 *
 * It also receives text shared with Weft (through the exported `.ShareTextActivity` alias); that
 * path sends the shared text and never touches the clipboard, so other apps cannot use the alias
 * to read it.
 */
class ClipboardSendActivity : Activity() {
    private var done = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            overrideActivityTransition(OVERRIDE_TRANSITION_OPEN, 0, 0)
            overrideActivityTransition(OVERRIDE_TRANSITION_CLOSE, 0, 0)
        }
        val launchedDirectly = intent.component?.className == ClipboardSendActivity::class.java.name
        if (!launchedDirectly || intent.action == Intent.ACTION_SEND) {
            done = true
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
        weft.clipboardSender.send(text)
        toast(R.string.toast_text_sent)
    }

    private fun sendClipboard() {
        val clip = getSystemService(ClipboardManager::class.java).primaryClip
        if (clip == null || clip.itemCount == 0) return toast(R.string.toast_clipboard_empty)
        // ClipDescription.EXTRA_IS_SENSITIVE is public from API 33, but password managers set the
        // same key on older versions too, so it is honoured everywhere.
        if (clip.description.extras?.getBoolean(EXTRA_IS_SENSITIVE) == true) {
            return toast(R.string.toast_sensitive)
        }
        val text = clip.getItemAt(0).text?.toString() ?: return toast(R.string.toast_clipboard_not_text)
        if (text.isEmpty()) return toast(R.string.toast_clipboard_empty)
        weft.clipboardSender.send(text)
        toast(R.string.toast_clipboard_sent)
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
