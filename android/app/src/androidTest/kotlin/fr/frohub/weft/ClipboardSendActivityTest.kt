package fr.frohub.weft

import android.app.Activity
import android.app.Application
import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.ComponentName
import android.content.Intent
import android.os.Bundle
import android.os.PersistableBundle
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.SdkSuppress
import androidx.test.platform.app.InstrumentationRegistry
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ClipboardSendActivityTest {
    @get:Rule
    val deviceReady = DeviceReadyRule()

    private val app = ApplicationProvider.getApplicationContext<WeftApplication>()
    private val sent = CopyOnWriteArrayList<String>()
    private lateinit var realSender: ClipboardSender

    @Before
    fun swapSender() {
        realSender = app.clipboardSender
        app.clipboardSender = ClipboardSender {
            sent += it
            SendResult.Sent(1)
        }
    }

    @After
    fun restoreSender() {
        app.clipboardSender = realSender
    }

    @Test
    fun sendsClipboardText() {
        setClip(ClipData.newPlainText("test", "Bonjour depuis Weft, ça marche"))

        launchAndAwaitFinish(ClipboardSendActivity.intent(app))

        assertEquals(listOf("Bonjour depuis Weft, ça marche"), awaitSent(1))
    }

    @Test
    fun sendsSharedTextWithoutReadingClipboard() {
        setClip(ClipData.newPlainText("test", "clipboard text"))
        val share = Intent(Intent.ACTION_SEND)
            .setComponent(ComponentName(app, SHARE_ALIAS))
            .setType("text/plain")
            .putExtra(Intent.EXTRA_TEXT, "shared text")
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

        launchAndAwaitFinish(share)

        assertEquals(listOf("shared text"), awaitSent(1))
    }

    @Test
    fun shareAliasNeverReadsClipboard() {
        setClip(ClipData.newPlainText("test", "clipboard text"))
        val direct = Intent()
            .setComponent(ComponentName(app, SHARE_ALIAS))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

        launchAndAwaitFinish(direct)

        assertEquals(emptyList<String>(), awaitSent(0))
    }

    @Test
    @SdkSuppress(minSdkVersion = 33)
    fun doesNotSendSensitiveClip() {
        val clip = ClipData.newPlainText("password", "hunter2")
        clip.description.extras = PersistableBundle().apply {
            putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true)
        }
        setClip(clip)

        launchAndAwaitFinish(ClipboardSendActivity.intent(app))

        assertEquals(emptyList<String>(), awaitSent(0))
    }

    /**
     * The activity hands the text to the sender in the background after it finishes: waits for
     * [count] sends (or a short while when none is expected), then returns what was sent.
     */
    private fun awaitSent(count: Int): List<String> {
        val deadline = System.currentTimeMillis() + if (count == 0) 500 else 5_000
        while (System.currentTimeMillis() < deadline && (count == 0 || sent.size < count)) Thread.sleep(20)
        return sent.toList()
    }

    private fun setClip(clip: ClipData) {
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            app.getSystemService(ClipboardManager::class.java).setPrimaryClip(clip)
        }
    }

    /** Starts the activity and waits until it has finished on its own. */
    private fun launchAndAwaitFinish(intent: Intent) {
        val destroyed = CountDownLatch(1)
        val callbacks = object : Application.ActivityLifecycleCallbacks {
            override fun onActivityDestroyed(activity: Activity) {
                if (activity is ClipboardSendActivity) destroyed.countDown()
            }
            override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) = Unit
            override fun onActivityStarted(activity: Activity) = Unit
            override fun onActivityResumed(activity: Activity) = Unit
            override fun onActivityPaused(activity: Activity) = Unit
            override fun onActivityStopped(activity: Activity) = Unit
            override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) = Unit
        }
        app.registerActivityLifecycleCallbacks(callbacks)
        try {
            app.startActivity(intent)
            // A system dialog popping up meanwhile (an "isn't responding" one, on a loaded
            // emulator) would keep the focus away: keep closing them while waiting.
            var finished = false
            repeat(10) {
                if (!finished) finished = destroyed.await(1, TimeUnit.SECONDS)
                if (!finished) DeviceReadyRule.closeSystemDialogs()
            }
            assertTrue("ClipboardSendActivity did not finish; focus: ${DeviceReadyRule.focus()}", finished)
        } finally {
            app.unregisterActivityLifecycleCallbacks(callbacks)
        }
    }

    private companion object {
        const val SHARE_ALIAS = "fr.frohub.weft.ShareTextActivity"
    }
}
