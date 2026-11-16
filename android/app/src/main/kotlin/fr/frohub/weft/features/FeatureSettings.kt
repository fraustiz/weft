package fr.frohub.weft.features

import android.content.Context
import androidx.core.content.edit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * One switch per feature, saved in shared preferences. On by default except SMS, contacts and
 * input (docs/features.md, section 9). Not secret and not integrity-sensitive, so it does not
 * live in the state file. The remote control's pointer speed is kept here too.
 */
class FeatureSettings(context: Context) {
    private val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private val _enabled = MutableStateFlow(read())
    private val _pointerSpeed = MutableStateFlow(prefs.getFloat(POINTER_SPEED, DEFAULT_POINTER_SPEED))

    val enabled: StateFlow<Set<Feature>> = _enabled.asStateFlow()

    /** How far the computer's pointer moves per pixel on the touchpad, before acceleration. */
    val pointerSpeed: StateFlow<Float> = _pointerSpeed.asStateFlow()

    fun isEnabled(feature: Feature) = feature in _enabled.value

    @Synchronized
    fun set(feature: Feature, on: Boolean) {
        prefs.edit { putBoolean(feature.key, on) }
        _enabled.value = read()
    }

    fun setPointerSpeed(speed: Float) {
        val value = speed.coerceIn(MIN_POINTER_SPEED, MAX_POINTER_SPEED)
        prefs.edit { putFloat(POINTER_SPEED, value) }
        _pointerSpeed.value = value
    }

    private fun read(): Set<Feature> = Feature.entries.filter { prefs.getBoolean(it.key, it.defaultOn) }.toSet()

    companion object {
        private const val PREFS = "features"
        private const val POINTER_SPEED = "pointer_speed"
        const val MIN_POINTER_SPEED = 0.5f
        const val MAX_POINTER_SPEED = 3f
        const val DEFAULT_POINTER_SPEED = 1.5f
    }
}
