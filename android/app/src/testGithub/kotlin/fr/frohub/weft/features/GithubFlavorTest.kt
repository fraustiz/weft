package fr.frohub.weft.features

import fr.frohub.weft.BuildConfig
import org.junit.Assert.assertTrue
import org.junit.Test

/** The GitHub build keeps everything: the updater, SMS, the call log and the full screen intent. */
class GithubFlavorTest {
    @Test
    fun keepsEverything() {
        assertTrue(BuildConfig.UPDATER_ENABLED)
        assertTrue(BuildConfig.SMS_ENABLED)
        assertTrue(BuildConfig.CALL_LOG_ENABLED)
        assertTrue(BuildConfig.FULL_SCREEN_INTENT_ENABLED)
        assertTrue(Feature.entries.all { it.available })
    }
}
