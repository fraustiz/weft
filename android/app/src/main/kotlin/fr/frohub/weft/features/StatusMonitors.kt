package fr.frohub.weft.features

import android.Manifest
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.BatteryManager
import android.os.Build
import android.telephony.TelephonyManager

/**
 * Reports the battery state through [onChange] while started: once at [start] (the broadcast
 * is sticky), then on every system change. The caller throttles.
 */
class BatteryMonitor(private val context: Context, private val onChange: (Packet.Battery) -> Unit) {
    private var receiver: BroadcastReceiver? = null

    fun start() {
        if (receiver != null) return
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                read(intent)?.let(onChange)
            }
        }
        this.receiver = receiver
        // A protected system broadcast: no export flag needed.
        context.registerReceiver(receiver, IntentFilter(Intent.ACTION_BATTERY_CHANGED))?.let { sticky ->
            read(sticky)?.let(onChange)
        }
    }

    fun stop() {
        receiver?.let { context.unregisterReceiver(it) }
        receiver = null
    }

    private fun read(intent: Intent): Packet.Battery? {
        val level = intent.getIntExtra(BatteryManager.EXTRA_LEVEL, -1)
        val scale = intent.getIntExtra(BatteryManager.EXTRA_SCALE, -1)
        if (level < 0 || scale <= 0) return null
        val status = intent.getIntExtra(BatteryManager.EXTRA_STATUS, BatteryManager.BATTERY_STATUS_UNKNOWN)
        val charging = status == BatteryManager.BATTERY_STATUS_CHARGING || status == BatteryManager.BATTERY_STATUS_FULL
        // The system's own low-battery flag exists from API 28; before that it is left out.
        val low = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P && intent.hasExtra(BatteryManager.EXTRA_BATTERY_LOW)) {
            intent.getBooleanExtra(BatteryManager.EXTRA_BATTERY_LOW, false)
        } else {
            null
        }
        return Packet.Battery((level * 100 / scale).coerceIn(0, 100), charging, low)
    }
}

/**
 * Reports the network this phone uses through [onChange] while started: Wi-Fi with its signal
 * level, mobile data with its generation (only with the phone permission, which Android requires
 * for it) and signal level, or none. Updated whenever the default network or its capabilities
 * change; the caller throttles.
 */
class ConnectivityMonitor(private val context: Context, private val onChange: (Packet.Connectivity) -> Unit) {
    private val connectivity = context.getSystemService(ConnectivityManager::class.java)
    private var callback: ConnectivityManager.NetworkCallback? = null

    fun start() {
        if (callback != null) return
        val callback = object : ConnectivityManager.NetworkCallback() {
            override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
                onChange(describe(capabilities))
            }

            override fun onLost(network: Network) {
                // Another default network may follow at once; that one is reported then.
                onChange(Packet.Connectivity(Packet.NETWORK_NONE, null))
            }
        }
        this.callback = callback
        connectivity.registerDefaultNetworkCallback(callback)
        val active = connectivity.activeNetwork?.let { connectivity.getNetworkCapabilities(it) }
        onChange(if (active == null) Packet.Connectivity(Packet.NETWORK_NONE, null) else describe(active))
    }

    fun stop() {
        callback?.let { connectivity.unregisterNetworkCallback(it) }
        callback = null
    }

    private fun describe(capabilities: NetworkCapabilities): Packet.Connectivity = when {
        capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> {
            val signal = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) wifiSignalLevel(capabilities.signalStrength) else null
            Packet.Connectivity(Packet.NETWORK_WIFI, signal)
        }
        capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> cellular()
        // Ethernet, VPN over something unknown, Bluetooth tethering: connected, type unknown.
        else -> Packet.Connectivity(null, null)
    }

    private fun cellular(): Packet.Connectivity {
        val telephony = context.getSystemService(TelephonyManager::class.java) ?: return Packet.Connectivity(null, null)
        val network = if (hasPhonePermission(context)) {
            try {
                cellularNetworkName(telephony.dataNetworkType)
            } catch (_: SecurityException) {
                null
            }
        } else {
            null
        }
        val signal = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) telephony.signalStrength?.level else null
        return Packet.Connectivity(network, signal?.coerceIn(0, 4))
    }

    companion object {
        /** Android asks for it to tell 5G from LTE; without it only Wi-Fi and "none" are known. */
        fun hasPhonePermission(context: Context) =
            context.checkSelfPermission(Manifest.permission.READ_PHONE_STATE) == PackageManager.PERMISSION_GRANTED
    }
}
