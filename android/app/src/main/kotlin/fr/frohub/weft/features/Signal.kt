package fr.frohub.weft.features

/**
 * The `network` value for a mobile data network type ([android.telephony.TelephonyManager]
 * `NETWORK_TYPE_*`), or null when it says nothing useful (unknown, IWLAN…). The values are
 * written out: some constants are newer than minSdk, and they never change.
 */
fun cellularNetworkName(networkType: Int): String? = when (networkType) {
    // NR
    20 -> Packet.NETWORK_5G
    // LTE, LTE_CA
    13, 19 -> Packet.NETWORK_LTE
    // UMTS, EVDO_0, EVDO_A, HSDPA, HSUPA, HSPA, EVDO_B, EHRPD, HSPAP, TD_SCDMA
    3, 5, 6, 8, 9, 10, 12, 14, 15, 17 -> Packet.NETWORK_3G
    // GPRS, EDGE, CDMA, 1xRTT, IDEN, GSM
    1, 2, 4, 7, 11, 16 -> Packet.NETWORK_2G
    else -> null
}

/**
 * A Wi-Fi RSSI (dBm) as a 0 to 4 level, with the thresholds Android's own five-bar scale has
 * long used. Null for the "unknown" values Android reports.
 */
fun wifiSignalLevel(rssi: Int): Int? = when {
    rssi >= 0 || rssi <= -127 -> null
    rssi >= -55 -> 4
    rssi >= -66 -> 3
    rssi >= -77 -> 2
    rssi >= -88 -> 1
    else -> 0
}
