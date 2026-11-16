package fr.frohub.weft

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService

/** Quick Settings tile that opens [ClipboardSendActivity], which reads and sends the clipboard. */
class ClipboardTileService : TileService() {

    override fun onStartListening() {
        super.onStartListening()
        val tile = qsTile ?: return
        tile.state = Tile.STATE_INACTIVE
        tile.updateTile()
    }

    override fun onClick() {
        super.onClick()
        // The clipboard can be read only behind an unlocked screen.
        if (isLocked) unlockAndRun { openSendActivity() } else openSendActivity()
    }

    // The Intent overload is only reached below API 34, where it is the only one.
    @SuppressLint("StartActivityAndCollapseDeprecated")
    private fun openSendActivity() {
        val intent = ClipboardSendActivity.intent(this)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            // From API 34 the Intent overload throws.
            val pending = PendingIntent.getActivity(
                this,
                0,
                intent,
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            )
            startActivityAndCollapse(pending)
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(intent)
        }
    }
}
