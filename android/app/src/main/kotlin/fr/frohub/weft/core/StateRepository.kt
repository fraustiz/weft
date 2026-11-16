package fr.frohub.weft.core

import android.util.Log
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import org.json.JSONException
import org.json.JSONObject

/**
 * The device state on disk: one JSON file, always replaced atomically (temporary file, fsync,
 * rename). Every change goes through [update], which runs under one mutex, so there is a single
 * writer and a change is on disk before [update] returns.
 */
class StateRepository(private val file: File) {
    private val mutex = Mutex()
    private val _state = MutableStateFlow(DeviceState())
    private var loaded = false

    /** The state as last loaded or saved. Holds [DeviceState()] until the first [load]. */
    val state: StateFlow<DeviceState> = _state.asStateFlow()

    /** Reads the file once; later calls return the state in memory. */
    suspend fun load(): DeviceState = mutex.withLock { loadLocked() }

    /**
     * Applies [change] to the current state and saves the result if it differs. Returns what
     * [change] returned. [change] runs under the lock: keep it short and free of I/O.
     */
    suspend fun <T> update(change: (DeviceState) -> Pair<DeviceState, T>): T = mutex.withLock {
        val before = loadLocked()
        val (after, result) = change(before)
        if (after != before) {
            withContext(Dispatchers.IO) { write(after) }
            _state.value = after
        }
        result
    }

    private suspend fun loadLocked(): DeviceState {
        if (!loaded) {
            _state.value = withContext(Dispatchers.IO) { read() }
            loaded = true
        }
        return _state.value
    }

    private fun read(): DeviceState {
        if (!file.exists()) return DeviceState()
        return try {
            DeviceState.fromJson(JSONObject(file.readText(Charsets.UTF_8)))
        } catch (e: JSONException) {
            setAside(e)
        } catch (e: IllegalArgumentException) {
            setAside(e)
        }
    }

    /**
     * Saves are atomic, so a bad file means outside damage. It is kept next to the original for
     * inspection and the device starts unpaired rather than staying stuck.
     */
    private fun setAside(cause: Exception): DeviceState {
        Log.w(TAG, "invalid state file, starting unpaired: ${cause.javaClass.simpleName}")
        file.renameTo(File(file.parentFile, file.name + ".invalid"))
        return DeviceState()
    }

    private fun write(state: DeviceState) {
        val dir = file.parentFile ?: throw IOException("state file has no directory")
        dir.mkdirs()
        val temp = File(dir, file.name + ".tmp")
        // A leftover from an interrupted save: we hold the lock, so it is stale.
        temp.delete()
        FileOutputStream(temp).use { out ->
            out.write((state.toJson().toString(2) + "\n").toByteArray(Charsets.UTF_8))
            out.fd.sync()
        }
        if (!temp.renameTo(file)) {
            temp.delete()
            throw IOException("could not replace the state file")
        }
        Log.d(TAG, "state saved (${state.devices.size} trusted device(s))")
    }

    private companion object {
        const val TAG = "WeftState"
    }
}
