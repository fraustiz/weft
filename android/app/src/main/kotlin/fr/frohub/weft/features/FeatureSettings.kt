package fr.frohub.weft.features

import android.content.Context
import androidx.core.content.edit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * One switch per feature, saved in shared preferences. All on by default (docs/features.md,
 * section 9). Not secret and not integrity-sensitive, so it does not live in the state file.
 */
class FeatureSettings(context: Context) {
    private val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private val _enabled = MutableStateFlow(read())

    val enabled: StateFlow<Set<Feature>> = _enabled.asStateFlow()

    fun isEnabled(feature: Feature) = feature in _enabled.value

    @Synchronized
    fun set(feature: Feature, on: Boolean) {
        prefs.edit { putBoolean(feature.key, on) }
        _enabled.value = read()
    }

    private fun read(): Set<Feature> = Feature.entries.filter { prefs.getBoolean(it.key, true) }.toSet()

    private companion object {
        const val PREFS = "features"
    }
}
