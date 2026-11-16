package fr.frohub.weft

import android.Manifest
import android.app.StatusBarManager
import android.content.ComponentName
import android.content.pm.PackageManager
import android.graphics.drawable.Icon
import android.os.Build
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.RequiresApi
import fr.frohub.weft.ui.CoreModel
import fr.frohub.weft.ui.WeftApp
import fr.frohub.weft.ui.WeftTheme

class MainActivity : ComponentActivity() {
    // The service notification (connection state, "Send clipboard") needs it from Android 13.
    private val notificationPermission = registerForActivityResult(ActivityResultContracts.RequestPermission()) {}

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val canAddTile = Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU
        val model = weft.model
        setContent {
            WeftTheme {
                WeftApp(
                    model = model,
                    onSendClipboard = { startActivity(ClipboardSendActivity.intent(this)) },
                    onAddTile = if (canAddTile) ::requestAddTile else null,
                    // Asked once there is something to notify about, not on first launch.
                    onPaired = ::requestNotificationPermission,
                )
            }
        }
    }

    override fun onStart() {
        super.onStart()
        weft.startServiceIfPaired()
    }

    private fun requestNotificationPermission() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return
        notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
    }

    @RequiresApi(Build.VERSION_CODES.TIRAMISU)
    private fun requestAddTile() {
        getSystemService(StatusBarManager::class.java).requestAddTileService(
            ComponentName(this, ClipboardTileService::class.java),
            getString(R.string.tile_label),
            Icon.createWithResource(this, R.drawable.ic_tile),
            mainExecutor,
        ) { result ->
            if (result == StatusBarManager.TILE_ADD_REQUEST_RESULT_TILE_ALREADY_ADDED) {
                Toast.makeText(this, R.string.tile_already_added, Toast.LENGTH_SHORT).show()
            }
        }
    }
}
