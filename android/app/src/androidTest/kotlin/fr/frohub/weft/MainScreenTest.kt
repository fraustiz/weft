package fr.frohub.weft

import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import fr.frohub.weft.ui.CAMERA_BLOCK
import fr.frohub.weft.ui.CODE_FIELD
import fr.frohub.weft.ui.PAIR_BUTTON
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** The real activity and core: the main action matches the pairing state on disk. */
@RunWith(AndroidJUnit4::class)
class MainScreenTest {
    @get:Rule(order = 0)
    val deviceReady = DeviceReadyRule()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    private val core get() = compose.activity.weft.core

    private fun paired(): Boolean = runBlocking {
        core.loaded.first { it }
        core.devices.value.isNotEmpty()
    }

    @Test
    fun showsTheMainAction() {
        val action = if (paired()) R.string.action_send_clipboard else R.string.action_pair
        compose.onNodeWithText(compose.activity.getString(action))
            .assertIsDisplayed()
            .assertHasClickAction()
    }

    @Test
    fun opensThePairingScreen() {
        assumeTrue("this device is paired", !paired())
        compose.onNodeWithTag(PAIR_BUTTON).performClick()
        compose.onNodeWithTag(CAMERA_BLOCK).assertIsDisplayed()
        compose.onNodeWithTag(CODE_FIELD).assertExists()
    }
}
