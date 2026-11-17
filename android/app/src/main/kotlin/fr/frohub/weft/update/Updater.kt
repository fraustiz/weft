package fr.frohub.weft.update

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.util.Log
import androidx.core.content.edit
import fr.frohub.weft.BuildConfig
import fr.frohub.weft.features.Notifier
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/** Where the updater stands, for the Updates section of the settings. */
sealed interface UpdateState {
    data object Idle : UpdateState
    data object Checking : UpdateState
    data object UpToDate : UpdateState
    data class Available(val info: UpdateInfo) : UpdateState

    /** Android must let Weft install apps first; the download starts once it does. */
    data class NeedsPermission(val info: UpdateInfo) : UpdateState

    /** [done] and [total] in bytes; [total] is -1 while unknown. */
    data class Downloading(val info: UpdateInfo, val done: Long, val total: Long) : UpdateState

    /** Handed to Android's installer, which asks the user to confirm. */
    data class Installing(val info: UpdateInfo) : UpdateState
    data class Failed(val error: UpdateError, val info: UpdateInfo?) : UpdateState
}

/**
 * Checks GitHub Releases for a newer Weft, downloads it to the cache, checks its hash and hands
 * it to Android's package installer. The APK is signed with the same key as the installed app,
 * so Android updates in place and keeps the pairing.
 *
 * Automatic checks run at most once a day, when the app or its service starts, and notify when
 * they find a version not notified yet.
 */
class Updater(private val context: Context) {
    private val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val client = UpdateClient(
        manifestUrl = BuildConfig.UPDATE_MANIFEST_URL,
        policy = if (BuildConfig.DEBUG) UpdatePolicy.forOverride(BuildConfig.UPDATE_MANIFEST_URL) else UpdatePolicy.GITHUB,
    )
    private val _state = MutableStateFlow<UpdateState>(UpdateState.Idle)
    private val _autoCheck = MutableStateFlow(prefs.getBoolean(AUTO_CHECK, true))
    private var job: Job? = null

    val state: StateFlow<UpdateState> = _state.asStateFlow()
    val autoCheck: StateFlow<Boolean> = _autoCheck.asStateFlow()

    val installedVersionName: String = packageInfo().versionName ?: "?"
    private val installedVersionCode: Long = packageInfo().let {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) it.longVersionCode else @Suppress("DEPRECATION") it.versionCode.toLong()
    }

    init {
        // An APK left by an install that never came back (the process died meanwhile).
        downloadDir().deleteRecursively()
    }

    fun setAutoCheck(on: Boolean) {
        prefs.edit { putBoolean(AUTO_CHECK, on) }
        _autoCheck.value = on
    }

    /** "Check for updates". */
    fun check() = launchIfIdle { runCheck(automatic = false) }

    /** On app or service start: checks if switched on and the last check is a day old. */
    fun checkIfDue() {
        if (!UpdateSchedule.isDue(_autoCheck.value, prefs.getLong(LAST_CHECK, 0), System.currentTimeMillis())) return
        launchIfIdle { runCheck(automatic = true) }
    }

    /** "Download and install": asks for the install permission first when Android needs it. */
    fun downloadAndInstall() {
        val info = when (val current = _state.value) {
            is UpdateState.Available -> current.info
            is UpdateState.NeedsPermission -> current.info
            is UpdateState.Failed -> current.info ?: return
            else -> return
        }
        if (!canInstall()) {
            _state.value = UpdateState.NeedsPermission(info)
            return
        }
        launchIfIdle { runDownload(info) }
    }

    /** Android's page to let Weft install apps. */
    fun permissionIntent(): Intent =
        Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${context.packageName}"))

    /** Back from that page (the activity resumes): goes on with the download if now allowed. */
    fun onResume() {
        val current = _state.value
        if (current is UpdateState.NeedsPermission && canInstall()) downloadAndInstall()
    }

    /** From [InstallResultReceiver]: Android's answer to the install session. */
    internal fun onInstallResult(status: Int, message: String?, confirm: Intent?) {
        val info = (_state.value as? UpdateState.Installing)?.info
        when (status) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                if (confirm == null) {
                    fail(UpdateError.Install("no confirmation screen"), info)
                    return
                }
                try {
                    context.startActivity(confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                } catch (e: RuntimeException) {
                    fail(UpdateError.Install(e.javaClass.simpleName), info)
                }
            }
            // On success Android replaces and stops this process; this is seldom reached.
            PackageInstaller.STATUS_SUCCESS -> _state.value = UpdateState.Idle
            PackageInstaller.STATUS_FAILURE_ABORTED -> _state.value = info?.let { UpdateState.Available(it) } ?: UpdateState.Idle
            else -> fail(UpdateError.Install(message ?: "status $status"), info)
        }
    }

    private fun launchIfIdle(block: suspend () -> Unit) {
        if (job?.isActive == true) return
        job = scope.launch {
            try {
                block()
            } catch (e: UpdateError) {
                fail(e, null)
            } catch (e: Exception) {
                Log.w(TAG, "update failed: ${e.javaClass.simpleName}")
                fail(UpdateError.Install(e.javaClass.simpleName), null)
            }
        }
    }

    private fun runCheck(automatic: Boolean) {
        val previous = _state.value
        if (!automatic) _state.value = UpdateState.Checking
        val info = try {
            client.fetchLatest()
        } catch (e: UpdateError) {
            // An automatic check fails quietly: no network at start-up is common.
            if (automatic) _state.value = previous else _state.value = UpdateState.Failed(e, null)
            return
        }
        prefs.edit { putLong(LAST_CHECK, System.currentTimeMillis()) }
        if (!info.isNewerThan(installedVersionCode)) {
            if (!automatic) _state.value = UpdateState.UpToDate
            return
        }
        _state.value = UpdateState.Available(info)
        if (automatic && prefs.getLong(LAST_NOTIFIED, 0) != info.versionCode) {
            prefs.edit { putLong(LAST_NOTIFIED, info.versionCode) }
            Notifier(context).updateAvailable(info.versionName)
        }
    }

    private fun runDownload(info: UpdateInfo) {
        val dir = downloadDir()
        dir.deleteRecursively()
        dir.mkdirs()
        val apk = File(dir, "weft-${info.versionCode}.apk")
        try {
            _state.value = UpdateState.Downloading(info, 0, -1)
            var lastPermille = -1L
            client.download(info, apk) { done, total ->
                // Updates the screen every 0.1 %, not for every buffer.
                val permille = if (total > 0) done * 1000 / total else done / (256 * 1024)
                if (permille != lastPermille) {
                    lastPermille = permille
                    _state.value = UpdateState.Downloading(info, done, total)
                }
            }
            checkApk(apk, info)
            _state.value = UpdateState.Installing(info)
            install(apk)
        } catch (e: UpdateError) {
            fail(e, info)
        } catch (e: Exception) {
            Log.w(TAG, "install failed: ${e.javaClass.simpleName}")
            fail(UpdateError.Install(e.javaClass.simpleName), info)
        } finally {
            // Copied into the install session, or failed: either way no longer needed.
            dir.deleteRecursively()
        }
    }

    /** The APK must be Weft, with the version the manifest announced. */
    private fun checkApk(apk: File, info: UpdateInfo) {
        val archive: PackageInfo = context.packageManager.getPackageArchiveInfo(apk.path, 0)
            ?: throw UpdateError.Install("not an APK")
        if (archive.packageName != context.packageName) throw UpdateError.Install("another app")
        val code = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) archive.longVersionCode else @Suppress("DEPRECATION") archive.versionCode.toLong()
        if (code != info.versionCode) throw UpdateError.Install("version mismatch")
    }

    private fun install(apk: File) {
        val installer = context.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
            setAppPackageName(context.packageName)
            setSize(apk.length())
        }
        val sessionId = installer.createSession(params)
        try {
            installer.openSession(sessionId).use { session ->
                apk.inputStream().use { input ->
                    session.openWrite("weft.apk", 0, apk.length()).use { output ->
                        input.copyTo(output, Sha256.BUFFER)
                        session.fsync(output)
                    }
                }
                val callback = PendingIntent.getBroadcast(
                    context,
                    sessionId,
                    Intent(context, InstallResultReceiver::class.java).setPackage(context.packageName),
                    // Mutable: the installer adds the status and the confirmation intent.
                    PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
                )
                session.commit(callback.intentSender)
            }
        } catch (e: Exception) {
            installer.abandonSession(sessionId)
            throw e
        }
    }

    private fun fail(error: UpdateError, info: UpdateInfo?) {
        Log.w(TAG, error.message ?: "update failed")
        _state.value = UpdateState.Failed(error, info)
    }

    private fun canInstall(): Boolean = context.packageManager.canRequestPackageInstalls()

    private fun downloadDir() = File(context.cacheDir, "updates")

    private fun packageInfo(): PackageInfo = context.packageManager.getPackageInfo(context.packageName, 0)

    private companion object {
        const val TAG = "WeftUpdate"
        const val PREFS = "updates"
        const val AUTO_CHECK = "auto_check"
        const val LAST_CHECK = "last_check"
        const val LAST_NOTIFIED = "last_notified"
    }
}
