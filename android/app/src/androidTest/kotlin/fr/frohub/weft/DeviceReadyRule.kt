package fr.frohub.weft

import android.Manifest
import android.os.Build
import androidx.test.platform.app.InstrumentationRegistry
import java.io.FileInputStream
import org.junit.rules.ExternalResource

/**
 * Wakes the device, dismisses the keyguard and closes system dialogs before each test: an
 * activity behind the lock screen or an "isn't responding" dialog (common on a headless
 * emulator) never gets window focus, so it can never read the clipboard. Also grants the
 * notification permission, so its dialog never covers the activity under test.
 */
class DeviceReadyRule : ExternalResource() {
    override fun before() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        shell("input keyevent KEYCODE_WAKEUP")
        shell("wm dismiss-keyguard")
        closeSystemDialogs()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            instrumentation.uiAutomation.grantRuntimePermission(
                instrumentation.targetContext.packageName,
                Manifest.permission.POST_NOTIFICATIONS,
            )
        }
        instrumentation.waitForIdleSync()
    }

    companion object {
        fun closeSystemDialogs() {
            shell("am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS")
        }

        /** The window that has the input focus, for failure messages. */
        fun focus(): String =
            shell("dumpsys window").lineSequence().firstOrNull { "mCurrentFocus" in it }?.trim().orEmpty()

        /** Runs a shell command as the shell user and waits for it to finish. */
        fun shell(command: String): String {
            val descriptor = InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand(command)
            return FileInputStream(descriptor.fileDescriptor).use { it.readBytes().toString(Charsets.UTF_8) }
                .also { descriptor.close() }
        }
    }
}
