package fr.frohub.weft

import android.Manifest
import android.app.StatusBarManager
import android.content.ActivityNotFoundException
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.drawable.Icon
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.RequiresApi
import androidx.compose.runtime.mutableStateOf
import androidx.lifecycle.lifecycleScope
import fr.frohub.weft.features.CallMonitor
import fr.frohub.weft.features.ConnectivityMonitor
import fr.frohub.weft.features.Feature
import fr.frohub.weft.features.WeftNotificationListener
import fr.frohub.weft.ui.CoreModel
import fr.frohub.weft.ui.WeftApp
import fr.frohub.weft.ui.WeftTheme
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    // The service notification (connection state, "Send clipboard") needs it from Android 13.
    private val notificationPermission = registerForActivityResult(ActivityResultContracts.RequestPermission()) {}

    // Received files go to the public Downloads folder with it, below Android 10 only.
    private val storagePermission = registerForActivityResult(ActivityResultContracts.RequestPermission()) {}

    // Only to tell 5G from LTE in the connectivity packet.
    private val phonePermission = registerForActivityResult(ActivityResultContracts.RequestPermission()) {
        phoneAccess.value = ConnectivityMonitor.hasPhonePermission(this)
    }

    private val phoneAccess = mutableStateOf(true)

    // Calls: phone state, then the number (call log) and the name (contacts), asked together.
    private val callPermissions = registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        updateCallAccess()
    }

    private val callAccess = mutableStateOf(true)

    /** Whether Weft may read notifications (Notifications and the phone's players in Media). */
    private val notificationAccess = mutableStateOf(true)

    /** The device the file picker was opened for; kept across a recreation. */
    private var fileTarget: String? = null

    private val pickFiles = registerForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        val target = fileTarget ?: return@registerForActivityResult
        fileTarget = null
        if (uris.isNotEmpty()) sendFiles(target, uris)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        fileTarget = savedInstanceState?.getString(STATE_FILE_TARGET)
        val canAddTile = Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU
        val model = weft.model
        setContent {
            WeftTheme {
                WeftApp(
                    model = model,
                    onSendClipboard = { startActivity(ClipboardSendActivity.intent(this)) },
                    onAddTile = if (canAddTile) ::requestAddTile else null,
                    // Asked once there is something to notify about, not on first launch.
                    onPaired = {
                        requestNotificationPermission()
                        requestStoragePermission()
                    },
                    onSendFile = { deviceId ->
                        fileTarget = deviceId
                        pickFiles.launch(arrayOf("*/*"))
                    },
                    onFeatureToggled = ::onFeatureToggled,
                    onAllowPhoneAccess = if (phoneAccess.value) null else ({ phonePermission.launch(Manifest.permission.READ_PHONE_STATE) }),
                    onAllowNotificationAccess = if (notificationAccess.value) null else ::openNotificationAccess,
                    onAllowCallAccess = if (callAccess.value) null else ({ callPermissions.launch(CallMonitor.permissions().toTypedArray()) }),
                )
            }
        }
    }

    override fun onStart() {
        super.onStart()
        weft.startServiceIfPaired()
    }

    override fun onResume() {
        super.onResume()
        // The user may have changed it in the system settings meanwhile.
        phoneAccess.value = ConnectivityMonitor.hasPhonePermission(this)
        notificationAccess.value = WeftNotificationListener.isAllowed(this)
        updateCallAccess()
    }

    /** Re-reads the call permissions; a change restarts the call monitor so it uses them. */
    private fun updateCallAccess() {
        val granted = CallMonitor.missingPermissions(this).isEmpty()
        val changed = granted != callAccess.value
        callAccess.value = granted
        weft.features.refreshMonitors(restartCalls = changed)
    }

    /** Android's page for notification access: Weft's own where the phone has one, the list otherwise. */
    private fun openNotificationAccess() {
        try {
            startActivity(WeftNotificationListener.settingsIntent(this))
        } catch (_: ActivityNotFoundException) {
            startActivity(Intent(Settings.ACTION_NOTIFICATION_LISTENER_SETTINGS))
        }
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        outState.putString(STATE_FILE_TARGET, fileTarget)
    }

    /** Opens the picked files now, while their read grant lasts, and sends them. */
    private fun sendFiles(deviceId: String, uris: List<Uri>) {
        lifecycleScope.launch {
            val message = ShareFileActivity.sendUris(this@MainActivity, deviceId, uris)
            Toast.makeText(this@MainActivity, message, Toast.LENGTH_SHORT).show()
        }
    }

    /** A feature switched on asks for the permission it needs, then and only then. */
    private fun onFeatureToggled(feature: Feature, on: Boolean) {
        if (!on) return
        when (feature) {
            Feature.CONNECTIVITY -> if (!ConnectivityMonitor.hasPhonePermission(this)) {
                phonePermission.launch(Manifest.permission.READ_PHONE_STATE)
            }
            Feature.FILES -> requestStoragePermission()
            Feature.RING, Feature.PING, Feature.LINKS -> requestNotificationPermission()
            Feature.NOTIFICATIONS, Feature.MEDIA -> if (!WeftNotificationListener.isAllowed(this)) openNotificationAccess()
            Feature.CALLS -> if (CallMonitor.missingPermissions(this).isNotEmpty()) {
                callPermissions.launch(CallMonitor.permissions().toTypedArray())
            }
            Feature.BATTERY, Feature.VOLUME -> Unit
        }
    }

    private fun requestNotificationPermission() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) return
        notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
    }

    private fun requestStoragePermission() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) return
        if (checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE) == PackageManager.PERMISSION_GRANTED) return
        storagePermission.launch(Manifest.permission.WRITE_EXTERNAL_STORAGE)
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

    private companion object {
        const val STATE_FILE_TARGET = "file_target"
    }
}
