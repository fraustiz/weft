package fr.frohub.weft.features

import android.app.Notification
import android.app.PendingIntent
import android.app.RemoteInput
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.service.notification.NotificationListenerService
import android.service.notification.StatusBarNotification
import android.util.Log
import androidx.core.app.NotificationManagerCompat
import fr.frohub.weft.weft

/**
 * Reads the notifications of this phone for the computers (docs/features.md, section 5), and acts
 * on them for them: dismiss, an action, an inline reply. Bound by Android once the user lets Weft
 * read notifications in the system settings; the same permission gives the media sessions
 * ([MediaWatcher]). What Android hides from listeners (one-time codes from Android 15) stays
 * hidden: Weft sends what it is given.
 */
class WeftNotificationListener : NotificationListenerService() {
    private val labels = HashMap<String, String>()

    override fun onListenerConnected() {
        weft.features.onListenerConnected(this)
    }

    override fun onListenerDisconnected() {
        weft.features.onListenerDisconnected(this)
    }

    override fun onNotificationPosted(sbn: StatusBarNotification) {
        weft.features.onNotificationPosted(read(sbn))
    }

    override fun onNotificationRemoved(sbn: StatusBarNotification) {
        weft.features.onNotificationRemoved(sbn.key)
    }

    /** Every notification shown now. Main thread. */
    fun active(): List<PostedNotification> = try {
        activeNotifications?.map(::read) ?: emptyList()
    } catch (e: SecurityException) {
        // The permission was taken back and the system has not unbound yet.
        Log.w(TAG, "could not read the notifications: ${e.javaClass.simpleName}")
        emptyList()
    }

    /** Dismisses [key] if the user could. Main thread. */
    fun dismiss(key: String) {
        val sbn = find(key) ?: return
        if (sbn.isClearable) cancelNotification(key)
    }

    /** Runs the plain action of [key] labelled [label]. False if there is none. Main thread. */
    fun runAction(key: String, label: String): Boolean {
        val action = find(key)?.notification?.actions?.firstOrNull {
            it.title?.toString() == label && it.remoteInputs.isNullOrEmpty()
        } ?: return false
        return send(action.actionIntent, null)
    }

    /** Sends [text] through the inline reply of [key]. False if it has none. Main thread. */
    fun reply(key: String, text: String): Boolean {
        val action = find(key)?.notification?.actions?.firstOrNull { it.replyInput() != null } ?: return false
        val inputs = action.remoteInputs
        val results = Bundle()
        for (input in inputs) if (input.allowFreeFormInput) results.putCharSequence(input.resultKey, text)
        val intent = Intent()
        RemoteInput.addResultsToIntent(inputs, intent, results)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) RemoteInput.setResultsSource(intent, RemoteInput.SOURCE_FREE_FORM_INPUT)
        return send(action.actionIntent, intent)
    }

    private fun send(pending: PendingIntent?, fillIn: Intent?): Boolean {
        pending ?: return false
        return try {
            pending.send(this, 0, fillIn)
            true
        } catch (_: PendingIntent.CanceledException) {
            false
        }
    }

    private fun find(key: String): StatusBarNotification? = try {
        getActiveNotifications(arrayOf(key))?.firstOrNull()
    } catch (_: SecurityException) {
        null
    }

    private fun read(sbn: StatusBarNotification): PostedNotification {
        val notification = sbn.notification
        val extras = notification.extras
        val title = extras.getCharSequence(Notification.EXTRA_TITLE_BIG) ?: extras.getCharSequence(Notification.EXTRA_TITLE)
        val text = extras.getCharSequence(Notification.EXTRA_BIG_TEXT) ?: extras.getCharSequence(Notification.EXTRA_TEXT)
        // Actions that need a choice from a list (no free text) cannot be done from a label alone.
        val actions = notification.actions.orEmpty().mapNotNull { action ->
            val label = action.title?.toString() ?: return@mapNotNull null
            when {
                action.replyInput() != null -> PostedAction(label, takesText = true)
                action.remoteInputs.isNullOrEmpty() -> PostedAction(label, takesText = false)
                else -> null
            }
        }
        return PostedNotification(
            key = sbn.key,
            packageName = sbn.packageName,
            appLabel = label(sbn.packageName),
            title = title?.toString(),
            text = text?.toString(),
            time = sbn.postTime,
            ongoing = sbn.isOngoing,
            groupSummary = (notification.flags and Notification.FLAG_GROUP_SUMMARY) != 0,
            clearable = sbn.isClearable,
            actions = actions,
        )
    }

    private fun label(packageName: String): String = labels.getOrPut(packageName) { appLabel(this, packageName) }

    private fun Notification.Action.replyInput(): RemoteInput? = remoteInputs?.firstOrNull { it.allowFreeFormInput }

    companion object {
        private const val TAG = "WeftNotifications"

        fun component(context: Context) = ComponentName(context, WeftNotificationListener::class.java)

        /** True once the user let Weft read notifications. */
        fun isAllowed(context: Context): Boolean =
            context.packageName in NotificationManagerCompat.getEnabledListenerPackages(context)

        /** The system screen where the user lets Weft read notifications: Weft's own page where Android has one. */
        fun settingsIntent(context: Context): Intent =
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                Intent(Settings.ACTION_NOTIFICATION_LISTENER_DETAIL_SETTINGS)
                    .putExtra(Settings.EXTRA_NOTIFICATION_LISTENER_COMPONENT_NAME, component(context).flattenToString())
            } else {
                Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS)
            }
    }
}

/** An app's name as the launcher shows it, or its package name when it cannot be read. */
fun appLabel(context: Context, packageName: String): String = try {
    val pm = context.packageManager
    pm.getApplicationLabel(pm.getApplicationInfo(packageName, 0)).toString()
} catch (_: PackageManager.NameNotFoundException) {
    packageName
}
