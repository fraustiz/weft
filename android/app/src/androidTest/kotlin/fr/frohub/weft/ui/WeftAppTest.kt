package fr.frohub.weft.ui

import android.content.Context
import androidx.compose.foundation.layout.Box
import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertTextContains
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
import fr.frohub.weft.core.InviteResult
import fr.frohub.weft.core.InviteState
import fr.frohub.weft.core.JoinResult
import fr.frohub.weft.core.JoinState
import fr.frohub.weft.core.RenameResult
import fr.frohub.weft.features.Packet
import fr.frohub.weft.features.PeerInfo
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
        // The computer announces its own name now: there is nothing to type for it.
        compose.onNodeWithText("Computer name", substring = true).assertDoesNotExist()

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

            assertEquals(code, model.joins.last())
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
    fun unpairedPhoneJoinsWithoutBeingAsked() {
        launch()
        compose.onNodeWithTag(PAIR_BUTTON).performClick()
        compose.onNodeWithTag(CODE_FIELD).performTextInput("WEFT1:abc")
        compose.onNodeWithTag(JOIN_BUTTON).performScrollTo().performClick()

        compose.onNodeWithTag(CHOICE_DIALOG).assertDoesNotExist()
        assertEquals(listOf("WEFT1:abc"), model.joins)
        assertTrue(model.invites.isEmpty())
    }

    @Test
    fun pairedPhoneIsAskedAndAddingIsTheDefault() {
        model.devices.value = listOf(LAPTOP, UNNAMED)
        launch()
        compose.onNodeWithTag(ADD_DEVICE_ROW).performScrollTo().assertHasClickAction().performClick()
        compose.onNodeWithText(string(R.string.action_add_device)).assertIsDisplayed()
        compose.onNodeWithTag(CODE_FIELD).performTextInput(" WEFT1:new ")
        compose.onNodeWithTag(JOIN_BUTTON).performScrollTo().performClick()

        // Nothing starts before the choice.
        compose.onNodeWithTag(CHOICE_DIALOG).assertIsDisplayed()
        assertTrue(model.joins.isEmpty() && model.invites.isEmpty())
        compose.onNodeWithText(string(R.string.invite_choice_title)).assertIsDisplayed()
        compose.onNodeWithText(context.resources.getQuantityString(R.plurals.invite_choice_body, 2, 2)).assertIsDisplayed()
        compose.onNodeWithTag(CHOICE_ADD).assertTextContains(string(R.string.invite_choice_add))
        compose.onNodeWithTag(CHOICE_JOIN).assertTextContains(string(R.string.invite_choice_join))
        // Leaving says what it costs.
        compose.onNodeWithText(context.resources.getQuantityString(R.plurals.invite_choice_join_hint, 2, 2)).assertIsDisplayed()

        model.nextInvite = InviteResult.Added(STUDIO)
        compose.onNodeWithTag(CHOICE_ADD).performClick()

        assertEquals(listOf("WEFT1:new"), model.invites)
        assertTrue(model.joins.isEmpty())
        compose.onNodeWithTag(CHOICE_DIALOG).assertDoesNotExist()
        compose.onNodeWithTag(INVITE_ADDED).assertIsDisplayed()
        compose.onNodeWithText(context.getString(R.string.invite_added_title, "Studio iMac")).assertIsDisplayed()
        compose.onNodeWithText(string(R.string.invite_added_body)).assertIsDisplayed()

        compose.onNodeWithTag(DONE_BUTTON).performClick()
        compose.onNodeWithTag(STATUS_CARD).assertIsDisplayed()
        compose.onNodeWithText("Studio iMac").performScrollTo().assertIsDisplayed()
        assertEquals(InviteState.Idle, model.inviteState.value)
        // Not a join: the paired callback is for the first pairing.
        assertEquals(0, pairedCalls)
    }

    @Test
    fun pairedPhoneCanStillLeaveForTheComputersGroup() {
        model.devices.value = listOf(LAPTOP)
        launch()
        compose.onNodeWithTag(ADD_DEVICE_ROW).performScrollTo().performClick()
        compose.onNodeWithTag(CODE_FIELD).performTextInput("WEFT1:theirs")
        compose.onNodeWithTag(JOIN_BUTTON).performScrollTo().performClick()
        compose.onNodeWithText(context.resources.getQuantityString(R.plurals.invite_choice_join_hint, 1, 1)).assertIsDisplayed()

        model.nextJoin = JoinResult.Joined(listOf(STUDIO), replacedGroup = true)
        compose.onNodeWithTag(CHOICE_JOIN).performClick()

        assertEquals(listOf("WEFT1:theirs"), model.joins)
        assertTrue(model.invites.isEmpty())
        compose.waitUntil(5_000) { pairedCalls == 1 }
        compose.onNodeWithText("Studio iMac").performScrollTo().assertIsDisplayed()
    }

    @Test
    fun cancellingTheChoiceStartsNothing() {
        model.devices.value = listOf(LAPTOP)
        launch()
        compose.onNodeWithTag(ADD_DEVICE_ROW).performScrollTo().performClick()
        compose.onNodeWithTag(CODE_FIELD).performTextInput("WEFT1:abc")
        compose.onNodeWithTag(JOIN_BUTTON).performScrollTo().performClick()
        compose.onNodeWithText(string(R.string.action_cancel)).performClick()

        compose.onNodeWithTag(CHOICE_DIALOG).assertDoesNotExist()
        compose.onNodeWithTag(CODE_FIELD).assertExists()
        assertTrue(model.joins.isEmpty() && model.invites.isEmpty())
        // Submitted by hand, the same code asks again.
        compose.onNodeWithTag(JOIN_BUTTON).performScrollTo().performClick()
        compose.onNodeWithTag(CHOICE_DIALOG).assertIsDisplayed()
    }

    @Test
    fun eachInvitationFailureIsExplained() {
        model.devices.value = listOf(LAPTOP)
        launch()
        compose.onNodeWithTag(ADD_DEVICE_ROW).performScrollTo().performClick()
        val failures = listOf(
            InviteResult.InvalidCode,
            InviteResult.OwnCode,
            InviteResult.OtherRelay,
            InviteResult.GroupFull,
            InviteResult.NoAnswer,
            InviteResult.Busy,
        ) + ConnectionProblem.entries.map { InviteResult.ConnectionFailed("test", it) } +
            ControlError.entries.map { InviteResult.Refused(it) }
        for ((index, failure) in failures.withIndex()) {
            model.nextInvite = failure
            val code = "WEFT1:code$index"
            compose.onNodeWithTag(CODE_FIELD).performScrollTo().performTextClearance()
            compose.onNodeWithTag(CODE_FIELD).performTextInput(code)
            compose.onNodeWithTag(JOIN_BUTTON).performScrollTo().performClick()
            compose.onNodeWithTag(CHOICE_ADD).performClick()

            assertEquals(code, model.invites.last())
            val (title, body) = inviteErrorText(failure)
            compose.onNodeWithTag(JOIN_ERROR).assertIsDisplayed()
            compose.onNodeWithText(string(title)).assertIsDisplayed()
            compose.onNodeWithText(string(body)).assertIsDisplayed()
            compose.onNodeWithTag(RETRY_BUTTON).performClick()
            compose.onNodeWithTag(CODE_FIELD).assertExists()
        }
        assertTrue(model.joins.isEmpty())
    }

    @Test
    fun noGroupOffersToPairInstead() {
        model.devices.value = listOf(LAPTOP)
        launch()
        compose.onNodeWithTag(ADD_DEVICE_ROW).performScrollTo().performClick()
        model.nextInvite = InviteResult.NoGroup
        compose.onNodeWithTag(CODE_FIELD).performTextInput("WEFT1:abc")
        compose.onNodeWithTag(JOIN_BUTTON).performScrollTo().performClick()
        compose.onNodeWithTag(CHOICE_ADD).performClick()

        compose.onNodeWithText(string(R.string.invite_no_group)).assertIsDisplayed()
        model.nextJoin = JoinResult.Joined(listOf(STUDIO), replacedGroup = false)
        compose.onNodeWithTag(JOIN_INSTEAD_BUTTON).assertTextContains(string(R.string.action_join_instead)).performClick()

        assertEquals(listOf("WEFT1:abc"), model.joins)
        compose.waitUntil(5_000) { pairedCalls == 1 }
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
    fun controlRowsAndTheirHintFollowTheComputersCapabilities() {
        model.devices.value = listOf(LAPTOP)
        model.peers.value = mapOf(LAPTOP.id to PeerInfo(Packet.Capabilities("windows", setOf(Packet.PING), setOf(Packet.PING, Packet.INPUT_KEY))))
        launch()
        compose.onNodeWithTag(DEVICE_ROW_PREFIX + LAPTOP.id).performScrollTo().performClick()
        val hint = context.getString(R.string.device_control_hint, "MacBook Air", "Pixel 8")
        compose.onNodeWithText(hint).performScrollTo().assertIsDisplayed()
        compose.onNodeWithTag(REMOTE_ROW).assertDoesNotExist()

        // The computer allows this phone: its new capabilities show the rows, no reopening.
        val allowed = setOf(Packet.PING, Packet.INPUT_POINTER, Packet.INPUT_CLICK, Packet.INPUT_KEY, Packet.PRESENTER)
        model.peers.value = mapOf(LAPTOP.id to PeerInfo(Packet.Capabilities("windows", allowed, setOf(Packet.PING, Packet.INPUT_KEY))))
        compose.onNodeWithTag(REMOTE_ROW).performScrollTo().assertIsDisplayed()
        compose.onNodeWithTag(PRESENTER_ROW).assertExists()
        compose.onNodeWithTag(CONTROL_HINT).assertDoesNotExist()
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

    @Test
    fun renamingThisPhoneChecksTheNameAsItIsTyped() {
        model.devices.value = listOf(LAPTOP)
        launch()
        compose.onNodeWithContentDescription(string(R.string.more_options)).performClick()
        compose.onNodeWithTag(RENAME_MENU_ITEM).performClick()
        compose.onNodeWithTag(RENAME_DIALOG).assertIsDisplayed()
        compose.onNodeWithText(string(R.string.rename_title)).assertIsDisplayed()
        // The current name, ready to edit.
        compose.onNodeWithTag(RENAME_FIELD).assertTextContains("Pixel 8")
        compose.onNodeWithTag(RENAME_SAVE).assertIsEnabled()

        val field = compose.onNodeWithTag(RENAME_FIELD)
        fun type(text: String) {
            field.performTextClearance()
            if (text.isNotEmpty()) field.performTextInput(text)
        }

        type("")
        compose.onNodeWithText(string(R.string.name_empty)).assertIsDisplayed()
        compose.onNodeWithTag(RENAME_SAVE).assertIsNotEnabled()
        // Spaces only: nothing left once trimmed.
        type("   ")
        compose.onNodeWithText(string(R.string.name_empty)).assertIsDisplayed()
        compose.onNodeWithTag(RENAME_SAVE).assertIsNotEnabled()

        type("a".repeat(65))
        compose.onNodeWithText(string(R.string.name_too_long)).assertIsDisplayed()
        compose.onNodeWithTag(RENAME_SAVE).assertIsNotEnabled()
        // 33 characters, but 66 bytes.
        type("é".repeat(33))
        compose.onNodeWithText(string(R.string.name_too_long)).assertIsDisplayed()
        // Exactly 64 bytes.
        type("é".repeat(32))
        compose.onNodeWithText(string(R.string.name_too_long)).assertDoesNotExist()
        compose.onNodeWithTag(RENAME_SAVE).assertIsEnabled()

        type("tab\there")
        compose.onNodeWithText(string(R.string.name_invalid_character)).assertIsDisplayed()
        compose.onNodeWithTag(RENAME_SAVE).assertIsNotEnabled()

        // Offline: saved here, and it says the other devices still show the old name.
        model.renameResult = RenameResult.NotSent
        type("  Pixel de test ")
        compose.onNodeWithTag(RENAME_SAVE).performClick()
        compose.onNodeWithText(string(R.string.rename_offline_body)).assertIsDisplayed()
        compose.onNodeWithText(string(R.string.action_try_again)).assertIsDisplayed()

        model.renameResult = RenameResult.Sent(1)
        compose.onNodeWithTag(RENAME_SAVE).performClick()
        compose.onNodeWithTag(RENAME_DIALOG).assertDoesNotExist()
        assertEquals(listOf("Pixel de test", "Pixel de test"), model.renames)
        assertEquals("Pixel de test", model.name.value)
    }

    @Test
    fun cancellingARenameChangesNothing() {
        model.devices.value = listOf(LAPTOP)
        launch()
        compose.onNodeWithContentDescription(string(R.string.more_options)).performClick()
        compose.onNodeWithTag(RENAME_MENU_ITEM).performClick()
        compose.onNodeWithTag(RENAME_FIELD).performTextClearance()
        compose.onNodeWithTag(RENAME_FIELD).performTextInput("Other")
        compose.onNodeWithText(string(R.string.action_cancel)).performClick()
        compose.onNodeWithTag(RENAME_DIALOG).assertDoesNotExist()
        assertTrue(model.renames.isEmpty())
        assertEquals("Pixel 8", model.name.value)
    }

    private fun string(id: Int) = context.getString(id)

    private class FakeModel : WeftModel {
        override val status = MutableStateFlow<ConnectionStatus>(ConnectionStatus.Connected)
        override val devices = MutableStateFlow<List<Device>?>(emptyList())
        override val lastSent = MutableStateFlow<LastSent?>(null)
        override val joinState = MutableStateFlow<JoinState>(JoinState.Idle)
        override val inviteState = MutableStateFlow<InviteState>(InviteState.Idle)
        override val name = MutableStateFlow("Pixel 8")
        override val peers = MutableStateFlow<Map<String, PeerInfo>>(emptyMap())
        val joins = CopyOnWriteArrayList<String>()
        val renames = CopyOnWriteArrayList<String>()
        var renameResult: RenameResult = RenameResult.Sent(1)
        val removed = CopyOnWriteArrayList<String>()
        val leaves = CopyOnWriteArrayList<Boolean>()
        var nextJoin: JoinResult = JoinResult.InvalidCode
        val invites = CopyOnWriteArrayList<String>()
        var nextInvite: InviteResult = InviteResult.NoAnswer
        var removeResult: GroupChangeResult = GroupChangeResult.Done

        override fun startJoin(code: String) {
            joins += code
            (nextJoin as? JoinResult.Joined)?.let { devices.value = it.devices }
            joinState.value = JoinState.Done(nextJoin)
        }

        override fun clearJoin() {
            if (joinState.value is JoinState.Done) joinState.value = JoinState.Idle
        }

        override fun startInvite(code: String) {
            invites += code
            (nextInvite as? InviteResult.Added)?.let { devices.value = devices.value.orEmpty() + it.device }
            inviteState.value = InviteState.Done(nextInvite)
        }

        override fun clearInvite() {
            if (inviteState.value is InviteState.Done) inviteState.value = InviteState.Idle
        }

        override suspend fun remove(deviceId: String): GroupChangeResult {
            removed += deviceId
            if (removeResult == GroupChangeResult.Done) devices.value = devices.value?.filterNot { it.id == deviceId }
            return removeResult
        }

        override suspend fun rename(name: String): RenameResult {
            renames += name
            if (renameResult !is RenameResult.Invalid) this.name.value = name
            return renameResult
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
        val STUDIO = Device("0c1d2e3f405162738495a6b7c8d9eaf0", "Studio iMac")
    }
}
