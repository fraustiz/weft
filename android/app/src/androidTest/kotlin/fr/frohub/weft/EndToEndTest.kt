package fr.frohub.weft

import android.app.NotificationManager
import android.content.ClipData
import android.content.ClipboardManager
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import fr.frohub.weft.core.ConnectionStatus
import fr.frohub.weft.core.JoinResult
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Steps of android/scripts/e2e.sh, against a real relay and the CLI. Each one runs only when
 * the script passes its instrumentation argument, so `connectedDebugAndroidTest` skips them.
 * They run in separate instrumentation runs (separate processes), so the pairing also goes
 * through the state saved on disk.
 */
@RunWith(AndroidJUnit4::class)
class EndToEndTest {
    @get:Rule
    val deviceReady = DeviceReadyRule()

    private val app = ApplicationProvider.getApplicationContext<WeftApplication>()
    private val args = InstrumentationRegistry.getArguments()

    /** `-e weftCode WEFT1:…`: joins the group of the CLI that printed the code. */
    @Test
    fun join() {
        val code = args.getString("weftCode")
        assumeTrue("no weftCode argument", code != null)
        ActivityScenario.launch(MainActivity::class.java).use {
            val result = runBlocking { app.core.join(code!!) }
            assertTrue("join failed: $result", result is JoinResult.Joined)
            assertEquals(1, (result as JoinResult.Joined).devices.size)
            awaitConnected()
        }
    }

    /**
     * `-e weftSendText …`: puts the text on the clipboard and taps "Send clipboard" in the
     * service notification, which opens ClipboardSendActivity.
     */
    @Test
    fun sendClipboard() {
        val text = args.getString("weftSendText")
        assumeTrue("no weftSendText argument", text != null)
        ActivityScenario.launch(MainActivity::class.java).use {
            awaitConnected()
            InstrumentationRegistry.getInstrumentation().runOnMainSync {
                app.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("e2e", text))
            }
            val notification = app.getSystemService(NotificationManager::class.java).activeNotifications
                .single { it.id == WeftService.NOTIFICATION_ID }
                .notification
            notification.actions.single().actionIntent.send()
            val sent = poll(10_000) { app.core.sender.lastSent.value }
            assertEquals(text!!.length, sent?.characterCount)
        }
    }

    /** `-e weftExpectText …`: waits until text sent by the CLI is on the clipboard. */
    @Test
    fun receiveClipboard() {
        val expected = args.getString("weftExpectText")
        assumeTrue("no weftExpectText argument", expected != null)
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            awaitConnected()
            // Read with the activity focused, as any foreground app would.
            val received = poll(60_000) {
                var clip: String? = null
                scenario.onActivity { activity ->
                    clip = activity.getSystemService(ClipboardManager::class.java)
                        .primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.text?.toString()
                }
                clip?.takeIf { it == expected }
            }
            assertEquals(expected, received)
        }
    }

    private fun awaitConnected() = runBlocking {
        withTimeout(30_000) { app.core.status.first { it == ConnectionStatus.Connected } }
    }

    private fun <T : Any> poll(timeoutMillis: Long, probe: () -> T?): T? {
        val deadline = System.currentTimeMillis() + timeoutMillis
        while (System.currentTimeMillis() < deadline) {
            probe()?.let { return it }
            Thread.sleep(250)
        }
        return null
    }
}
