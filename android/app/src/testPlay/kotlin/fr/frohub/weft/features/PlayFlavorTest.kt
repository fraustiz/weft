package fr.frohub.weft.features

import fr.frohub.weft.BuildConfig
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The Google Play build: no updater, no SMS, no call log, no full screen intent. */
class PlayFlavorTest {
    @Test
    fun leavesOutWhatPlayRestricts() {
        assertFalse(BuildConfig.UPDATER_ENABLED)
        assertFalse(BuildConfig.SMS_ENABLED)
        assertFalse(BuildConfig.CALL_LOG_ENABLED)
        assertFalse(BuildConfig.FULL_SCREEN_INTENT_ENABLED)
    }

    @Test
    fun smsIsNeverAdvertised() {
        assertFalse(Feature.SMS.available)
        // What FeatureSettings can switch on: every available feature.
        val caps = capabilitiesOf(Feature.entries.filter { it.available }.toSet())
        for (type in Feature.SMS.incoming) assertFalse(type in caps.incoming)
        for (type in Feature.SMS.outgoing) assertFalse(type in caps.outgoing)
        assertTrue(Packet.CALL in caps.outgoing)
    }
}
