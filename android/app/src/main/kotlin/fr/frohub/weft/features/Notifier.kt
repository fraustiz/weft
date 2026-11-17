package fr.frohub.weft.features

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import androidx.core.net.toUri
import fr.frohub.weft.R
import java.util.concurrent.atomic.AtomicInteger

/**
 * The notifications of this batch: ring, ping, links and file transfers, each kind on its own
 * channel so the user can tune them apart. Every one is skipped quietly while notifications are
 * not allowed.
 */
class Notifier(private val context: Context) {
    private val manager = context.getSystemService(NotificationManager::class.java)
    private val nextId = AtomicInteger(FIRST_MESSAGE_ID)

    init {
        manager.createNotificationChannels(
            listOf(
                NotificationChannel(CHANNEL_RING, context.getString(R.string.channel_ring), NotificationManager.IMPORTANCE_HIGH).apply {
                    // The sound comes from the alarm stream (Ringer), not from the notification.
                    setSound(null, null)
                    setShowBadge(false)
                },
                NotificationChannel(CHANNEL_MESSAGES, context.getString(R.string.channel_messages), NotificationManager.IMPORTANCE_HIGH),
                NotificationChannel(CHANNEL_TRANSFERS, context.getString(R.string.channel_transfers), NotificationManager.IMPORTANCE_LOW).apply {
                    setShowBadge(false)
                },
                NotificationChannel(CHANNEL_FILES, context.getString(R.string.channel_files), NotificationManager.IMPORTANCE_DEFAULT),
                NotificationChannel(CHANNEL_UPDATES, context.getString(R.string.channel_updates), NotificationManager.IMPORTANCE_DEFAULT),
            ),
        )
    }

    // region Ring

    /**
     * The "Find my phone" notice: full screen over the lock screen where Android allows it (from
     * Android 14 only if the user lets Weft), a heads-up notification otherwise.
     */
    fun ring(deviceLabel: String) {
        val open = PendingIntent.getActivity(
            context,
            REQUEST_RING,
            Intent(context, RingActivity::class.java)
                .putExtra(RingActivity.EXTRA_DEVICE, deviceLabel)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_USER_ACTION),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val stop = FeatureReceiver.stopRing(context)
        val fullScreen = Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE || manager.canUseFullScreenIntent()
        val notification = Notification.Builder(context, CHANNEL_RING)
            .setSmallIcon(R.drawable.ic_tile)
            .setContentTitle(context.getString(R.string.ring_title))
            .setContentText(context.getString(R.string.ring_body, deviceLabel))
            .setCategory(Notification.CATEGORY_ALARM)
            .setContentIntent(open)
            .setDeleteIntent(stop)
            .setOngoing(true)
            .setAutoCancel(true)
            .setTimeoutAfter(Ringer.RING_MILLIS)
            .addAction(Notification.Action.Builder(null, context.getString(R.string.ring_found), stop).build())
            .apply { if (fullScreen) setFullScreenIntent(open, true) }
            .build()
        notify(ID_RING, notification)
    }

    fun cancelRing() = manager.cancel(ID_RING)

    // endregion

    // region Ping and links

    fun ping(deviceLabel: String, message: String?) {
        val notification = Notification.Builder(context, CHANNEL_MESSAGES)
            .setSmallIcon(R.drawable.ic_tile)
            .setContentTitle(context.getString(R.string.ping_title, deviceLabel))
            .apply { if (!message.isNullOrBlank()) setContentText(message) }
            .setContentIntent(openApp())
            .setAutoCancel(true)
            .setCategory(Notification.CATEGORY_MESSAGE)
            .build()
        notify(nextId.getAndIncrement(), notification)
    }

    /** Opens the link only when tapped: Android does not let a background app start an activity. */
    fun link(deviceLabel: String, url: String) {
        val id = nextId.getAndIncrement()
        val view = Intent(Intent.ACTION_VIEW, url.toUri())
            .addCategory(Intent.CATEGORY_BROWSABLE)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        val notification = Notification.Builder(context, CHANNEL_MESSAGES)
            .setSmallIcon(R.drawable.ic_tile)
            .setContentTitle(context.getString(R.string.link_title, deviceLabel))
            .setContentText(url)
            .setStyle(Notification.BigTextStyle().bigText(url))
            .setContentIntent(PendingIntent.getActivity(context, id, view, PendingIntent.FLAG_IMMUTABLE))
            .setAutoCancel(true)
            .build()
        notify(id, notification)
    }

    // endregion

    // region Files

    /** Progress of a transfer, with a Cancel action. [done] and [size] in bytes. */
    fun transferProgress(transferId: String, deviceIdHex: String, incoming: Boolean, name: String, deviceLabel: String, done: Long, size: Long) {
        val title = if (incoming) R.string.file_receiving else R.string.file_sending
        val permille = if (size == 0L) 1000 else (done * 1000 / size).toInt()
        val notification = Notification.Builder(context, CHANNEL_TRANSFERS)
            .setSmallIcon(R.drawable.ic_tile)
            .setContentTitle(context.getString(title, name))
            .setContentText(deviceLabel)
            .setProgress(1000, permille, false)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setShowWhen(false)
            .setCategory(Notification.CATEGORY_PROGRESS)
            .addAction(
                Notification.Action.Builder(
                    null,
                    context.getString(R.string.action_cancel),
                    FeatureReceiver.cancelTransfer(context, deviceIdHex, transferId),
                ).build(),
            )
            .build()
        notify(transferNotificationId(transferId), notification)
    }

    /** "Received <name>"; tapping it opens the file with the app the user picks for its type. */
    fun received(transferId: String, name: String, deviceLabel: String, uri: Uri, mime: String?) {
        val view = Intent(Intent.ACTION_VIEW)
            .setDataAndType(uri, mime ?: "*/*")
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        val id = transferNotificationId(transferId)
        val notification = Notification.Builder(context, CHANNEL_FILES)
            .setSmallIcon(R.drawable.ic_tile)
            .setContentTitle(context.getString(R.string.file_received, name))
            .setContentText(context.getString(R.string.file_received_body, deviceLabel))
            .setContentIntent(PendingIntent.getActivity(context, id, view, PendingIntent.FLAG_IMMUTABLE))
            .setAutoCancel(true)
            .build()
        notify(id, notification)
    }

    /** A transfer that ended without the file: [message] says why. */
    fun transferEnded(transferId: String, title: String, message: String) {
        val notification = Notification.Builder(context, CHANNEL_FILES)
            .setSmallIcon(R.drawable.ic_tile)
            .setContentTitle(title)
            .setContentText(message)
            .setContentIntent(openApp())
            .setAutoCancel(true)
            .build()
        notify(transferNotificationId(transferId), notification)
    }

    fun cancelTransfer(transferId: String) = manager.cancel(transferNotificationId(transferId))

    // endregion

    // region Updates

    /** An automatic check found [versionName]; tapping opens the Updates section of the settings. */
    fun updateAvailable(versionName: String) {
        val open = PendingIntent.getActivity(
            context,
            REQUEST_UPDATE,
            Intent(context, fr.frohub.weft.MainActivity::class.java)
                .putExtra(fr.frohub.weft.MainActivity.EXTRA_SHOW_UPDATES, true)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val notification = Notification.Builder(context, CHANNEL_UPDATES)
            .setSmallIcon(R.drawable.ic_tile)
            .setContentTitle(context.getString(R.string.update_available, versionName))
            .setContentText(context.getString(R.string.update_notification_body))
            .setContentIntent(open)
            .setAutoCancel(true)
            .build()
        notify(ID_UPDATE, notification)
    }

    // endregion

    private fun openApp(): PendingIntent = PendingIntent.getActivity(
        context,
        REQUEST_OPEN,
        Intent(context, fr.frohub.weft.MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    private fun notify(id: Int, notification: Notification) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        manager.notify(id, notification)
    }

    private companion object {
        const val CHANNEL_RING = "ring"
        const val CHANNEL_MESSAGES = "messages"
        const val CHANNEL_TRANSFERS = "transfers"
        const val CHANNEL_FILES = "files"
        const val CHANNEL_UPDATES = "updates"

        // 1 is the service notification (WeftService.NOTIFICATION_ID).
        const val ID_RING = 2
        const val ID_UPDATE = 3
        const val FIRST_MESSAGE_ID = 1000
        const val REQUEST_RING = 10
        const val REQUEST_OPEN = 11
        const val REQUEST_UPDATE = 12

        /** Above every other id; one per transfer. */
        fun transferNotificationId(transferId: String) = 0x40000000 or (transferId.hashCode() and 0x3fffffff)
    }
}
