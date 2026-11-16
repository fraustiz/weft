package fr.frohub.weft.features

import android.content.Context
import androidx.core.content.edit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import org.json.JSONException
import org.json.JSONObject

/** An app that posted a notification on this phone lately: its package, label and when. */
data class SeenApp(val packageName: String, val label: String, val lastSeen: Long)

/**
 * The apps whose notifications stay on this phone, and the apps that posted some lately (for the
 * list the user picks from). Saved in shared preferences: package names and labels only, never
 * the content of a notification.
 */
class NotificationApps(context: Context) {
    private val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private val _excluded = MutableStateFlow(prefs.getStringSet(KEY_EXCLUDED, null)?.toSet() ?: emptySet())
    private val _seen = MutableStateFlow(decodeSeen(prefs.getString(KEY_SEEN, null)))

    val excluded: StateFlow<Set<String>> = _excluded.asStateFlow()

    /** Newest first. */
    val seen: StateFlow<List<SeenApp>> = _seen.asStateFlow()

    @Synchronized
    fun setExcluded(packageName: String, excluded: Boolean) {
        val next = if (excluded) _excluded.value + packageName else _excluded.value - packageName
        prefs.edit { putStringSet(KEY_EXCLUDED, next) }
        _excluded.value = next
    }

    /** Records that [packageName] posted a notification at [nowMillis]; saved at most once an hour per app. */
    @Synchronized
    fun saw(packageName: String, label: String, nowMillis: Long) {
        val before = _seen.value
        val known = before.firstOrNull { it.packageName == packageName }
        if (known != null && known.label == label && nowMillis - known.lastSeen < SAVE_INTERVAL_MILLIS) return
        val next = pruneSeen(before.filter { it.packageName != packageName } + SeenApp(packageName, label, nowMillis), nowMillis)
        prefs.edit { putString(KEY_SEEN, encodeSeen(next)) }
        _seen.value = next
    }

    private companion object {
        const val PREFS = "notification_apps"
        const val KEY_EXCLUDED = "excluded"
        const val KEY_SEEN = "seen"
        const val SAVE_INTERVAL_MILLIS = 60 * 60 * 1000L
    }
}

/** Apps seen in the last 30 days, at most 100, newest first. */
fun pruneSeen(apps: List<SeenApp>, nowMillis: Long): List<SeenApp> =
    apps.filter { nowMillis - it.lastSeen <= SEEN_FOR_MILLIS }
        .sortedByDescending { it.lastSeen }
        .take(MAX_SEEN_APPS)

fun encodeSeen(apps: List<SeenApp>): String {
    val json = JSONObject()
    for (app in apps) json.put(app.packageName, JSONObject().put("label", app.label).put("at", app.lastSeen))
    return json.toString()
}

/** What [encodeSeen] wrote; anything unreadable is dropped. */
fun decodeSeen(text: String?): List<SeenApp> {
    if (text == null) return emptyList()
    val json = try {
        JSONObject(text)
    } catch (_: JSONException) {
        return emptyList()
    }
    return json.keys().asSequence().mapNotNull { name ->
        val entry = json.optJSONObject(name) ?: return@mapNotNull null
        val label = entry.opt("label") as? String ?: return@mapNotNull null
        val at = (entry.opt("at") as? Number)?.toLong() ?: return@mapNotNull null
        SeenApp(name, label, at)
    }.sortedByDescending { it.lastSeen }.toList()
}

private const val SEEN_FOR_MILLIS = 30L * 24 * 60 * 60 * 1000
private const val MAX_SEEN_APPS = 100
