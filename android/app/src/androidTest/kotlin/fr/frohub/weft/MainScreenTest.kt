package fr.frohub.weft

import androidx.compose.ui.test.assertHasClickAction
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import fr.frohub.weft.ui.JOIN_CODE_FIELD
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class MainScreenTest {
    @get:Rule(order = 0)
    val deviceReady = DeviceReadyRule()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun showsSendClipboardAction() {
        compose.onNodeWithText(compose.activity.getString(R.string.action_send_clipboard))
            .assertIsDisplayed()
            .assertHasClickAction()
    }

    @Test
    fun showsJoinField() {
        compose.onNodeWithTag(JOIN_CODE_FIELD).assertIsDisplayed()
        compose.onNodeWithText(compose.activity.getString(R.string.action_join)).assertIsDisplayed()
    }
}
