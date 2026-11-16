package fr.frohub.weft.features

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import fr.frohub.weft.weft

/**
 * Notification actions that need no screen: "Found it" (and dismissing the ring notice) and
 * cancelling a transfer. Not exported: only this app's pending intents reach it.
 */
class FeatureReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val features = context.weft.features
        when (intent.action) {
            ACTION_STOP_RING -> features.stopRinging()
            ACTION_CANCEL_TRANSFER -> {
                val device = intent.getStringExtra(EXTRA_DEVICE) ?: return
                val id = intent.getStringExtra(EXTRA_TRANSFER) ?: return
                features.cancelTransfer(device, id)
            }
        }
    }

    companion object {
        private const val ACTION_STOP_RING = "fr.frohub.weft.action.STOP_RING"
        private const val ACTION_CANCEL_TRANSFER = "fr.frohub.weft.action.CANCEL_TRANSFER"
        private const val EXTRA_DEVICE = "device"
        private const val EXTRA_TRANSFER = "transfer"

        fun stopRing(context: Context): PendingIntent = PendingIntent.getBroadcast(
            context,
            0,
            Intent(context, FeatureReceiver::class.java).setAction(ACTION_STOP_RING),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        fun cancelTransfer(context: Context, deviceIdHex: String, transferId: String): PendingIntent = PendingIntent.getBroadcast(
            context,
            transferId.hashCode(),
            Intent(context, FeatureReceiver::class.java)
                .setAction(ACTION_CANCEL_TRANSFER)
                // Distinct data, so each transfer keeps its own pending intent.
                .setData(android.net.Uri.fromParts("weft", transferId, null))
                .putExtra(EXTRA_DEVICE, deviceIdHex)
                .putExtra(EXTRA_TRANSFER, transferId),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
    }
}
