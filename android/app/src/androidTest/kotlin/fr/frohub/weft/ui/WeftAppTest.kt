package fr.frohub.weft.ui

import android.content.Context
import androidx.compose.foundation.layout.Box
import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import fr.frohub.weft.DeviceReadyRule
import fr.frohub.weft.LastSent
import fr.frohub.weft.R
import fr.frohub.weft.core.ConnectionProblem
import fr.frohub.weft.core.ConnectionStatus
import fr.frohub.weft.core.Device
import fr.frohub.weft.core.GroupChangeResult
import fr.frohub.weft.core.JoinResult
import fr.frohub.weft.core.JoinState
import fr.frohub.weft.protocol.ControlError
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** The screens against a fake core: what they show for each state, and what they ask for. */
@RunWith(AndroidJUnit4::class)
class WeftAppTest {
    @get:Rule(order = 0)
    val deviceReady = DeviceReadyRule()

    @get:Rule(order = 1)
    val compose = createComposeRule()

    private val context = ApplicationProvider.getApplicationContext<Context>()
    private val model = FakeModel()
    private var pairedCalls = 0

    private fun launch() {
        compose.setContent {
            WeftTheme {
                WeftApp(
                    model = model,
                    onSendClipboard = {},
                    onAddTile = null,
                    onPaired = { pairedCalls++ },
                    // Never the real camera in these tests.
                    scanner = { _, _, modifier -> Box(modifier) },
                )
            }
        }
    }

    @Test
    fun unpairedShowsWhatWeftDoesAndHowToPair() {
        launch()
        compose.onNodeWithText(string(R.string.empty_title)).assertIsDisplayed()
        compose.onNodeWithText(string(R.string.empty_body)).assertIsDisplayed()
        compose.onNodeWithTag(PAIR_BUTTON).assertHasClickAction()
        compose.onNodeWithText(string(R.string.action_send_clipboard)).assertDoesNotExist()
        compose.onNodeWithContentDescription(string(R.string.more_options)).assertDoesNotExist()
    }

    @Test
    fun pastedCodeJoinsAndEachFailureIsExplained() {
        launch()
        compose.onNodeWithTag(PAIR_BUTTON).performClick()
        compose.onNodeWithTag(JOIN_BUTTON).performScrollTo().assertIsNotEnabled()
        compose.onNodeWithTag(NAME_FIELD).performTextInput("Work laptop")

        val failures = listOf(
            JoinResult.InvalidCode,
            JoinResult.OwnCode,
            JoinResult.InsecureRelay,
            JoinResult.TimedOut,
            JoinResult.Busy,
        ) + ConnectionProblem.entries.map { JoinResult.ConnectionFailed("test", it) } +
            ControlError.entries.map { JoinResult.Refused(it) }
        for ((index, failure) in failures.withIndex()) {
            model.nextJoin = failure
            val code = "WEFT1:code$index"
            compose.onNodeWithTag(CODE_FIELD).performScrollTo().performTextClearance()
            compose.onNodeWithTag(CODE_FIELD).performTextInput("  $code ")
            compose.onNodeWithTag(JOIN_BUTTON).performScrollTo().performClick()

            assertEquals(code to "Work laptop", model.joins.last())
            val (title, body) = joinErrorText(failure)
            compose.onNodeWithTag(JOIN_ERROR).assertIsDisplayed()
            compose.onNodeWithText(string(title)).assertIsDisplayed()
            compose.onNodeWithText(string(body)).assertIsDisplayed()
            compose.onNodeWithTag(RETRY_BUTTON).performClick()
            compose.onNodeWithTag(CODE_FIELD).assertExists()
        }
        assertEquals(failures.size, model.joins.size)
        assertEquals(0, pairedCalls)
    }

    @Test
    fun successfulJoinGoesBackToTheDevices() {
        launch()
        compose.onNodeWithTag(PAIR_BUTTON).performClick()
        model.nextJoin = JoinResult.Joined(listOf(LAPTOP), replacedGroup = false)
        compose.onNodeWithTag(CODE_FIELD).performTextInput("WEFT1:abc")
        compose.onNodeWithTag(JOIN_BUTTON).performScrollTo().performClick()

        compose.waitUntil(5_000) { pairedCalls == 1 }
        compose.onNodeWithTag(STATUS_CARD).assertIsDisplayed()
        compose.onNodeWithText("MacBook Air").performScrollTo().assertIsDisplayed()
        assertEquals(JoinState.Idle, model.joinState.value)
    }

    @Test
    fun pairedShowsTheConnectionAndTheDevices() {
        model.devices.value = listOf(LAPTOP, UNNAMED)
        model.lastSent.value = LastSent(System.currentTimeMillis(), 42)
        launch()

        compose.onNodeWithText(string(R.string.status_connected)).assertIsDisplayed()
        compose.onNodeWithText(context.resources.getQuantityString(R.plurals.paired_devices, 2, 2)).assertIsDisplayed()
        compose.onNodeWithText(string(R.string.action_send_clipboard)).assertHasClickAction()
        compose.onNodeWithText("MacBook Air").performScrollTo().assertIsDisplayed()
        // Named: the short id underneath. Unnamed: the short id is the name.
        compose.onNodeWithText(LAPTOP.shortId).assertIsDisplayed()
        compose.onNodeWithText(UNNAMED.shortId).performScrollTo().assertIsDisplayed()

        model.status.value = ConnectionStatus.Error("refused", ConnectionProblem.UNREACHABLE)
        compose.onNodeWithText(string(R.string.status_reconnecting)).assertIsDisplayed()
        compose.onNodeWithText(context.getString(R.string.status_retrying, string(R.string.problem_unreachable)))
            .assertIsDisplayed()
        // Still retrying, with the same reason, until it connects.
        model.status.value = ConnectionStatus.Connecting
        compose.onNodeWithText(string(R.string.status_reconnecting)).assertIsDisplayed()
        model.status.value = ConnectionStatus.Connected
        compose.onNodeWithText(string(R.string.status_connected)).assertIsDisplayed()
    }

    @Test
    fun removingADeviceAsksFirst() {
        model.devices.value = listOf(LAPTOP, UNNAMED)
        launch()
        compose.onNodeWithTag(DEVICE_ROW_PREFIX + LAPTOP.id).performScrollTo().performClick()
        compose.onNodeWithText(string(R.string.device_id)).assertIsDisplayed()
        compose.onNodeWithText(LAPTOP.shortId).assertIsDisplayed()

        // Cancel: nothing happens.
        compose.onNodeWithTag(REMOVE_BUTTON).performClick()
        compose.onNodeWithText(context.getString(R.string.remove_title, "MacBook Air")).assertIsDisplayed()
        compose.onNodeWithText(string(R.string.action_cancel)).performClick()
        compose.onNodeWithTag(CONFIRM_DIALOG).assertDoesNotExist()
        assertTrue(model.removed.isEmpty())

        // Offline: says so and offers to try again.
        model.removeResult = GroupChangeResult.NotConnected
        compose.onNodeWithTag(REMOVE_BUTTON).performClick()
        compose.onNodeWithTag(CONFIRM_BUTTON).performClick()
        compose.onNodeWithText(string(R.string.remove_offline_body)).assertIsDisplayed()

        model.removeResult = GroupChangeResult.Done
        compose.onNodeWithText(string(R.string.action_try_again)).performClick()
        compose.waitUntil(5_000) { model.devices.value?.size == 1 }
        assertEquals(listOf(LAPTOP.id, LAPTOP.id), model.removed)
        compose.onNodeWithTag(STATUS_CARD).assertIsDisplayed()
        compose.onNodeWithTag(DEVICE_ROW_PREFIX + LAPTOP.id).assertDoesNotExist()
    }

    @Test
    fun unpairingFromTheMenu() {
        model.devices.value = listOf(LAPTOP)
        launch()
        compose.onNodeWithContentDescription(string(R.string.more_options)).performClick()
        compose.onNodeWithText(string(R.string.action_leave)).performClick()
        compose.onNodeWithText(string(R.string.leave_title)).assertIsDisplayed()
        compose.onNodeWithTag(CONFIRM_BUTTON).performClick()

        compose.waitUntil(5_000) { model.devices.value?.isEmpty() == true }
        compose.onNodeWithText(string(R.string.empty_title)).assertIsDisplayed()
        assertEquals(listOf(false), model.leaves)
    }

    private fun string(id: Int) = context.getString(id)

    private class FakeModel : WeftModel {
        override val status = MutableStateFlow<ConnectionStatus>(ConnectionStatus.Connected)
        override val devices = MutableStateFlow<List<Device>?>(emptyList())
        override val lastSent = MutableStateFlow<LastSent?>(null)
        override val joinState = MutableStateFlow<JoinState>(JoinState.Idle)
        val joins = CopyOnWriteArrayList<Pair<String, String?>>()
        val removed = CopyOnWriteArrayList<String>()
        val leaves = CopyOnWriteArrayList<Boolean>()
        var nextJoin: JoinResult = JoinResult.InvalidCode
        var removeResult: GroupChangeResult = GroupChangeResult.Done

        override fun startJoin(code: String, name: String?) {
            joins += code to name
            (nextJoin as? JoinResult.Joined)?.let { devices.value = it.devices }
            joinState.value = JoinState.Done(nextJoin)
        }

        override fun clearJoin() {
            if (joinState.value is JoinState.Done) joinState.value = JoinState.Idle
        }

        override suspend fun remove(deviceId: String): GroupChangeResult {
            removed += deviceId
            if (removeResult == GroupChangeResult.Done) devices.value = devices.value?.filterNot { it.id == deviceId }
            return removeResult
        }

        override suspend fun leave(evenIfOffline: Boolean): GroupChangeResult {
            leaves += evenIfOffline
            devices.value = emptyList()
            return GroupChangeResult.Done
        }
    }

    private companion object {
        val LAPTOP = Device("3f9a0c12d4e5f60718293a4b5c6d7e8f", "MacBook Air", 1_763_000_000_000)
        val UNNAMED = Device("b7e14c09aa21f3d4e5f60718293a4b5c", null)
    }
}
