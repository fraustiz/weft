package fr.frohub.weft

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import fr.frohub.weft.core.ConnectionStatus
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch

/**
 * Foreground service (type `connectedDevice`) that keeps the relay connection open while this
 * device is paired, so text from the other devices lands on the clipboard at any time.
 *
 * Started by the app (launch, after a join), never at boot: Android 15 forbids starting a
 * `connectedDevice` service from BOOT_COMPLETED for apps targeting it, so after a reboot the
 * connection comes back the next time the app is opened. Stops itself when no trusted device
 * remains. Its persistent, low-importance notification shows the connection state and has a
 * "Send clipboard" action that opens [ClipboardSendActivity] directly (no trampoline).
 */
class WeftService : Service() {
    private var watcher: Job? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        createChannel(this)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val core = weft.core
        try {
            goForeground(notification(core.status.value))
        } catch (e: IllegalStateException) {
            // ForegroundServiceStartNotAllowedException (API 31+) is an IllegalStateException.
            Log.w(TAG, "not allowed to run in the foreground now: ${e.javaClass.simpleName}")
            stopSelf()
            return START_NOT_STICKY
        }
        core.keepConnected(true)
        if (watcher == null) {
            watcher = core.scope.launch {
                var hadDevices = false
                combine(core.status, core.devices) { status, devices -> status to devices.size }
                    .distinctUntilChanged()
                    .collect { (status, count) ->
                        if (count > 0) hadDevices = true
                        if (count == 0 && (hadDevices || core.repository.load().devices.isEmpty())) {
                            Log.i(TAG, "no trusted device left, stopping")
                            stopSelf()
                        } else {
                            update(notification(status))
                        }
                    }
            }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        watcher?.cancel()
        watcher = null
        weft.core.keepConnected(false)
        super.onDestroy()
    }

    private fun goForeground(notification: Notification) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    private fun update(notification: Notification) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        getSystemService(NotificationManager::class.java).notify(NOTIFICATION_ID, notification)
    }

    private fun notification(status: ConnectionStatus): Notification {
        val text = if (status == ConnectionStatus.Connected) R.string.notification_connected else R.string.notification_reconnecting
        val open = PendingIntent.getActivity(
            this,
            REQUEST_OPEN,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        // Straight to the activity: notification trampolines are blocked since Android 12.
        val send = PendingIntent.getActivity(
            this,
            REQUEST_SEND,
            ClipboardSendActivity.intent(this),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        return Notification.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_tile)
            .setContentTitle(getString(R.string.app_name))
            .setContentText(getString(text))
            .setContentIntent(open)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setShowWhen(false)
            .setCategory(Notification.CATEGORY_SERVICE)
            .addAction(
                Notification.Action.Builder(null, getString(R.string.action_send_clipboard), send).build(),
            )
            .apply {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    setForegroundServiceBehavior(Notification.FOREGROUND_SERVICE_IMMEDIATE)
                }
            }
            .build()
    }

    companion object {
        private const val TAG = "WeftService"
        private const val CHANNEL_ID = "connection"
        const val NOTIFICATION_ID = 1
        private const val REQUEST_OPEN = 0
        private const val REQUEST_SEND = 1

        /** Starts the service. Only allowed while the app is in the foreground. */
        fun start(context: Context) {
            context.startForegroundService(Intent(context, WeftService::class.java))
        }

        private fun createChannel(context: Context) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                context.getString(R.string.notification_channel),
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                setShowBadge(false)
            }
            context.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
        }
    }
}
