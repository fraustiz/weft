package fr.frohub.weft

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.annotation.StringRes
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.core.content.IntentCompat
import androidx.lifecycle.lifecycleScope
import fr.frohub.weft.core.Device
import fr.frohub.weft.features.Feature
import fr.frohub.weft.features.OutgoingFile
import fr.frohub.weft.features.Packet
import fr.frohub.weft.ui.WeftTheme
import fr.frohub.weft.ui.label
import fr.frohub.weft.ui.quietTextButtonColors
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Share target for files of any type (one or several). With one device that takes files, sends
 * at once; with several, asks which one first. Opens every file before finishing: the read grant
 * of a shared uri ends with this activity, an open file does not.
 *
 * A share that only carries text (the `*` filter matches text too) goes to the text share target.
 */
class ShareFileActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val app = weft
        app.startServiceIfPaired()
        val uris = sharedUris(intent)
        if (uris.isEmpty()) {
            if (intent.hasExtra(Intent.EXTRA_TEXT)) {
                startActivity(Intent(intent).setComponent(ComponentName(this, "$packageName.ShareTextActivity")))
            } else {
                toast(R.string.toast_no_file)
            }
            return finish()
        }
        if (!app.features.settings.isEnabled(Feature.FILES)) {
            toast(R.string.toast_files_off)
            return finish()
        }
        lifecycleScope.launch {
            val peers = app.features.peers.value
            // Every trusted device, except those known not to take files.
            val devices = app.core.repository.load().devices
                .filter { peers[it.deviceIdHex]?.capabilities == null || peers[it.deviceIdHex]?.handles(Packet.FILE_OFFER) == true }
                .map { Device(it.deviceIdHex, it.name) }
            when (devices.size) {
                0 -> {
                    toast(R.string.toast_no_file_device)
                    finish()
                }
                1 -> send(devices.single(), uris)
                else -> setContent { WeftTheme { DevicePicker(devices, onPick = { send(it, uris) }, onDismiss = ::finish) } }
            }
        }
    }

    private var sending = false

    private fun send(device: Device, uris: List<Uri>) {
        if (sending) return
        sending = true
        lifecycleScope.launch {
            toast(sendUris(this@ShareFileActivity, device.id, uris))
            finish()
        }
    }

    private fun toast(@StringRes message: Int) {
        Toast.makeText(applicationContext, message, Toast.LENGTH_SHORT).show()
    }

    companion object {
        private fun sharedUris(intent: Intent): List<Uri> = when (intent.action) {
            Intent.ACTION_SEND -> listOfNotNull(IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java))
            Intent.ACTION_SEND_MULTIPLE ->
                IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java).orEmpty().filterNotNull()
            else -> emptyList()
        }

        /**
         * Opens [uris] (off the main thread) and starts sending them to [deviceIdHex]. Call it
         * while the activity that got the uris is alive. Returns the toast to show (a string resource).
         */
        suspend fun sendUris(context: Context, deviceIdHex: String, uris: List<Uri>): Int {
            val files = withContext(Dispatchers.IO) { uris.mapNotNull { OutgoingFile.open(context.contentResolver, it) } }
            if (files.isEmpty()) return R.string.toast_file_unreadable
            context.weft.features.sendFiles(deviceIdHex, files)
            return if (files.size < uris.size) R.string.toast_files_some_unreadable else R.string.toast_files_sending
        }
    }
}

@Composable
private fun DevicePicker(devices: List<Device>, onPick: (Device) -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.pick_device_title)) },
        text = {
            Column {
                for (device in devices) {
                    ListItem(
                        headlineContent = { Text(device.label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                        leadingContent = { Icon(painterResource(R.drawable.ic_computer), contentDescription = null) },
                        colors = ListItemDefaults.colors(containerColor = Color.Transparent),
                        modifier = Modifier.clickable(role = Role.Button) { onPick(device) },
                    )
                }
            }
        },
        confirmButton = {},
        dismissButton = {
            TextButton(onClick = onDismiss, colors = quietTextButtonColors()) { Text(stringResource(R.string.action_cancel)) }
        },
    )
}
