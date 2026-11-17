package fr.frohub.weft.features

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.InsetDrawable
import android.graphics.drawable.RippleDrawable
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
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import fr.frohub.weft.R
import fr.frohub.weft.weft

/**
 * The Weft keyboard (docs/features.md, section 8): an input method with no keys, which types
 * into the focused field what a computer sends as `input.key`. Android binds it only while the
 * user has picked it, so a computer can never type into this phone otherwise. Its bar says who
 * types, or why nobody can, and switches back to a keyboard with keys. Plain views: an input method has no activity for
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
        val message = TextView(this).apply {
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            maxLines = 2
            ellipsize = TextUtils.TruncateAt.END
        }
        // A tonal button, like the app's: the way back to a keyboard with keys.
        val icon = getDrawable(R.drawable.ic_keyboard)!!.mutate().apply {
            setTint(getColor(R.color.keyboard_text))
            setBounds(0, 0, dp(18), dp(18))
        }
        val shape = GradientDrawable().apply {
            cornerRadius = dp(20).toFloat()
            setColor(getColor(R.color.keyboard_button))
        }
        val pressed = ColorStateList.valueOf(getColor(R.color.keyboard_text)).withAlpha(PRESSED_ALPHA)
        val switch = TextView(this).apply {
            text = getString(R.string.keyboard_switch)
            setTextColor(getColor(R.color.keyboard_text))
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
            maxLines = 1
            gravity = Gravity.CENTER_VERTICAL
            setCompoundDrawablesRelative(icon, null, null, null)
            compoundDrawablePadding = dp(8)
            setPaddingRelative(dp(12), 0, dp(16), 0)
            // 40 dp drawn in a 48 dp touch target.
            background = RippleDrawable(pressed, InsetDrawable(shape, 0, dp(4), 0, dp(4)), null)
            isClickable = true
            isFocusable = true
            setOnClickListener { switchKeyboard() }
        }
        status = message
        refresh()
        val row = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPaddingRelative(dp(16), 0, dp(8), 0)
            addView(message, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f).apply { marginEnd = dp(12) })
            addView(switch, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, dp(48)))
        }
        val bar = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(getColor(R.color.keyboard_bar))
            addView(row, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, dp(56)))
        }
        // Input method windows are drawn under the navigation bar (edge to edge from Android 15):
        // the bar's colour runs under it, its content stays above the gesture handle.
        ViewCompat.setOnApplyWindowInsetsListener(bar) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.navigationBars() or WindowInsetsCompat.Type.displayCutout())
            view.setPadding(bars.left, 0, bars.right, bars.bottom)
            insets
        }
        return bar
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

    /**
     * Main thread: the bar says whether a computer can type here now, and who typed last. Called
     * again when the Input switch, the relay connection or the computers' capabilities change.
     */
    fun refresh() {
        val text = status ?: return
        val features = weft.features
        val from = lastFrom
        val message = when {
            !features.settings.isEnabled(Feature.INPUT) -> R.string.keyboard_off
            !features.computerCanType() -> R.string.keyboard_no_computer
            from != null -> null
            else -> R.string.keyboard_waiting
        }
        text.text = if (message == null) getString(R.string.keyboard_typing_from, from) else getString(message)
        text.setTextColor(getColor(if (message == null) R.color.keyboard_text else R.color.keyboard_text_secondary))
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
        /** The pressed state of the switch button: the text colour at 12 %. */
        private const val PRESSED_ALPHA = 31

        /** Whether the user turned the Weft keyboard on in Android's keyboard settings. */
        fun isEnabled(context: Context): Boolean {
            val ours = context.packageName
            return context.getSystemService(InputMethodManager::class.java).enabledInputMethodList.any { it.packageName == ours }
        }

        /** Android's list of keyboards, where the Weft keyboard is turned on. */
        fun settingsAction(): String = Settings.ACTION_INPUT_METHOD_SETTINGS
    }
}
