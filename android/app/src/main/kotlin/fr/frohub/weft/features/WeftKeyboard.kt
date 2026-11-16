package fr.frohub.weft.features

import android.content.Context
import android.inputmethodservice.InputMethodService
import android.os.Build
import android.os.SystemClock
import android.provider.Settings
import android.text.TextUtils
import android.util.TypedValue
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager
import android.widget.ImageButton
import android.widget.LinearLayout
import android.widget.TextView
import fr.frohub.weft.R
import fr.frohub.weft.weft

/**
 * The Weft keyboard (docs/features.md, section 8): an input method with no keys, which types
 * into the focused field what a computer sends as `input.key`. Android binds it only while the
 * user has picked it, so a computer can never type into this phone otherwise. Its bar says who
 * types and switches back to another keyboard. Plain views: an input method has no activity for
 * Compose to hang on.
 */
class WeftKeyboard : InputMethodService() {
    private var status: TextView? = null
    private var typing = false
    private var lastFrom: String? = null

    override fun onCreate() {
        super.onCreate()
        weft.features.onKeyboardCreated(this)
    }

    override fun onDestroy() {
        weft.features.onKeyboardDestroyed(this)
        super.onDestroy()
    }

    override fun onCreateInputView(): View {
        val dp = { value: Int -> TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value.toFloat(), resources.displayMetrics).toInt() }
        val text = TextView(this).apply {
            setTextColor(getColor(R.color.keyboard_text))
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
        }
        val switch = ImageButton(this).apply {
            setImageResource(R.drawable.ic_keyboard)
            setColorFilter(getColor(R.color.keyboard_text_secondary))
            contentDescription = getString(R.string.keyboard_switch)
            background = null
            minimumWidth = dp(48)
            minimumHeight = dp(48)
            setOnClickListener { switchKeyboard() }
        }
        status = text
        refresh()
        return LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setBackgroundColor(getColor(R.color.keyboard_bar))
            setPadding(dp(16), dp(4), dp(4), dp(4))
            addView(text, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
            addView(switch, LinearLayout.LayoutParams(dp(48), dp(48)))
        }
    }

    override fun onStartInput(attribute: EditorInfo?, restarting: Boolean) {
        super.onStartInput(attribute, restarting)
        typing = true
    }

    override fun onFinishInput() {
        typing = false
        super.onFinishInput()
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        refresh()
    }

    /**
     * Main thread: types [packet], sent by [from], into the focused field. False when no field
     * takes text now, or the packet holds nothing this keyboard can type.
     */
    fun type(packet: Packet.InputKey, from: String): Boolean {
        val connection = currentInputConnection?.takeIf { typing } ?: return false
        lastFrom = from
        refresh()
        val meta = metaStateOf(packet)
        val key = packet.key
        if (key != null) {
            val code = keyCodeOf(key) ?: return false
            // A plain Enter runs the field's action (send, search, next) like a real keyboard.
            if (code == KeyEvent.KEYCODE_ENTER && meta == 0) sendKeyChar('\n') else sendKey(connection, code, meta)
            return true
        }
        val text = packet.text ?: return false
        // Ctrl+C and the like are shortcuts, not text.
        val shortcut = if (packet.ctrl || packet.alt || packet.meta) keyCodeOfChar(text) else null
        if (shortcut != null) sendKey(connection, shortcut, meta) else connection.commitText(text, 1)
        return true
    }

    /** Main thread: the bar's text follows the Input switch and the last computer that typed. */
    fun refresh() {
        val text = status ?: return
        text.text = when {
            !weft.features.settings.isEnabled(Feature.INPUT) -> getString(R.string.keyboard_off)
            lastFrom != null -> getString(R.string.keyboard_typing_from, lastFrom)
            else -> getString(R.string.keyboard_waiting)
        }
    }

    private fun sendKey(connection: InputConnection, code: Int, meta: Int) {
        val now = SystemClock.uptimeMillis()
        connection.sendKeyEvent(KeyEvent(now, now, KeyEvent.ACTION_DOWN, code, 0, meta))
        connection.sendKeyEvent(KeyEvent(now, now, KeyEvent.ACTION_UP, code, 0, meta))
    }

    private fun switchKeyboard() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P && switchToPreviousInputMethod()) return
        getSystemService(InputMethodManager::class.java).showInputMethodPicker()
    }

    companion object {
        /** Whether the user turned the Weft keyboard on in Android's keyboard settings. */
        fun isEnabled(context: Context): Boolean {
            val ours = context.packageName
            return context.getSystemService(InputMethodManager::class.java).enabledInputMethodList.any { it.packageName == ours }
        }

        /** Android's list of keyboards, where the Weft keyboard is turned on. */
        fun settingsAction(): String = Settings.ACTION_INPUT_METHOD_SETTINGS
    }
}
