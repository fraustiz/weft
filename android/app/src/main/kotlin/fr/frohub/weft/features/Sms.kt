package fr.frohub.weft.features

import android.Manifest
import android.app.Activity
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.database.Cursor
import android.os.Build
import android.provider.ContactsContract
import android.provider.Telephony
import android.telephony.SmsManager
import android.util.Log
import androidx.core.content.ContextCompat
import java.util.concurrent.atomic.AtomicInteger

private const val TAG = "WeftSms"

private fun Context.has(permission: String) = checkSelfPermission(permission) == PackageManager.PERMISSION_GRANTED

/**
 * This phone's SMS for the computers (docs/features.md, section 7): reads threads and messages
 * from Android's SMS table, sends through [SmsManager], and reports received messages while
 * [startReceiving] is on. Text SMS only: MMS are not listed in this revision. Every read is
 * blocking: call it off the main thread.
 */
class Sms(private val context: Context) {
    private var receiver: BroadcastReceiver? = null
    private val requestCodes = AtomicInteger()

    /** The newest threads, with contact names when Weft may read the contacts. */
    fun conversations(): Packet.SmsConversations {
        val unread = HashSet<Long>()
        query(
            arrayOf(Telephony.Sms.THREAD_ID),
            "${Telephony.Sms.READ} = 0 AND ${Telephony.Sms.TYPE} = $SMS_TYPE_INBOX",
            null,
            null,
        ) { unread += it.getLong(0) }
        val rows = ArrayList<SmsRow>()
        val threads = HashSet<Long>()
        // Newest first; the scan stops at the first row of a thread past the limit.
        query(ROW_COLUMNS, "${Telephony.Sms.TYPE} != $SMS_TYPE_DRAFT", null, "${Telephony.Sms.DATE} DESC") { cursor ->
            val row = cursor.toRow()
            rows += row
            threads += row.thread
            threads.size <= Packet.MAX_CONVERSATIONS && rows.size < MAX_SCANNED_ROWS
        }
        val contacts = context.has(Manifest.permission.READ_CONTACTS)
        return conversationsPacket(
            conversationsOf(rows.asSequence(), unread, nameOf = { if (contacts) CallMonitor.contactName(context, it) else null }),
        )
    }

    /** Up to [request]'s limit of messages of one thread before its time, oldest first. */
    fun messages(request: Packet.SmsMessagesRequest): Packet.SmsMessages {
        val selection = StringBuilder("${Telephony.Sms.THREAD_ID} = ? AND ${Telephony.Sms.TYPE} != $SMS_TYPE_DRAFT")
        val args = arrayListOf(request.thread.toString())
        request.before?.let {
            selection.append(" AND ${Telephony.Sms.DATE} < ?")
            args += it.toString()
        }
        val messages = ArrayList<SmsMessage>()
        query(ROW_COLUMNS, selection.toString(), args.toTypedArray(), "${Telephony.Sms.DATE} DESC") { cursor ->
            cursor.toRow().toMessage()?.let { messages += it }
            messages.size < request.limit
        }
        return messagesPacket(request.thread, messages.asReversed())
    }

    /**
     * The row Android stored for a message just received with [body], once the SMS app has
     * stored it; null while it has not.
     */
    fun findReceived(body: String, receivedAfter: Long): SmsRow? {
        var found: SmsRow? = null
        query(
            ROW_COLUMNS,
            "${Telephony.Sms.TYPE} = $SMS_TYPE_INBOX AND ${Telephony.Sms.BODY} = ? AND ${Telephony.Sms.DATE} >= ?",
            arrayOf(body, receivedAfter.toString()),
            "${Telephony.Sms.DATE} DESC",
        ) { cursor ->
            found = cursor.toRow()
            false
        }
        return found
    }

    /** The thread Android files messages with [address] under; null if it cannot tell. */
    fun threadOf(address: String): Long? = try {
        Telephony.Threads.getOrCreateThreadId(context, address)
    } catch (e: RuntimeException) {
        // IllegalArgumentException or SecurityException, depending on the phone.
        Log.w(TAG, "could not find a thread: ${e.javaClass.simpleName}")
        null
    }

    /**
     * Sends [text] to each address, in several parts when it is long. Returns how many results
     * [FeatureReceiver] will report for this request (one per part), or the error that stopped
     * it before anything left.
     */
    fun send(deviceIdHex: String, request: Packet.SmsSend): Result<Int> {
        if (!context.has(Manifest.permission.SEND_SMS)) return Result.failure(SmsError(ERROR_PERMISSION))
        val manager = manager() ?: return Result.failure(SmsError(ERROR_NO_SMS))
        var parts = 0
        return try {
            val divided = manager.divideMessage(request.text)
            for (address in request.addresses) {
                val sent = ArrayList<PendingIntent>()
                repeat(divided.size) {
                    sent += FeatureReceiver.smsSent(context, requestCodes.incrementAndGet(), deviceIdHex, request.requestId)
                }
                // Counted before it leaves: a result can arrive before this returns.
                parts += divided.size
                manager.sendMultipartTextMessage(address, null, divided, sent, null)
            }
            Result.success(parts)
        } catch (e: RuntimeException) {
            // A bad address (IllegalArgumentException) or the permission revoked meanwhile.
            Log.w(TAG, "could not send an SMS: ${e.javaClass.simpleName}")
            if (parts == 0) Result.failure(SmsError(ERROR_FAILED)) else Result.success(parts)
        }
    }

    /** Reports each received SMS (its parts joined) through [onReceived]. Main thread. */
    fun startReceiving(onReceived: (address: String, body: String, time: Long) -> Unit) {
        if (receiver != null || !context.has(Manifest.permission.RECEIVE_SMS)) return
        val created = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                if (intent.action != Telephony.Sms.Intents.SMS_RECEIVED_ACTION) return
                val parts = Telephony.Sms.Intents.getMessagesFromIntent(intent) ?: return
                // One SMS in several parts, or several SMS in one broadcast: joined per sender.
                parts.filterNotNull().groupBy { it.originatingAddress.orEmpty() }.forEach { (address, messages) ->
                    val body = messages.joinToString("") { it.messageBody.orEmpty() }
                    onReceived(address, body, messages.first().timestampMillis)
                }
            }
        }
        // Only senders holding BROADCAST_SMS (the system) reach it. Registered while the service
        // runs: a message can only reach the computers while connected anyway.
        ContextCompat.registerReceiver(
            context,
            created,
            IntentFilter(Telephony.Sms.Intents.SMS_RECEIVED_ACTION),
            Manifest.permission.BROADCAST_SMS,
            null,
            ContextCompat.RECEIVER_EXPORTED,
        )
        receiver = created
    }

    fun stopReceiving() {
        receiver?.let { context.unregisterReceiver(it) }
        receiver = null
    }

    @Suppress("DEPRECATION")
    private fun manager(): SmsManager? = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        context.getSystemService(SmsManager::class.java)
    } else {
        SmsManager.getDefault()
    }

    /** Runs [each] on the rows of the SMS table until it returns false. Nothing when not allowed. */
    private fun query(
        columns: Array<String>,
        selection: String?,
        args: Array<String>?,
        order: String?,
        each: (Cursor) -> Any,
    ) {
        if (!context.has(Manifest.permission.READ_SMS)) return
        try {
            context.contentResolver.query(Telephony.Sms.CONTENT_URI, columns, selection, args, order)?.use { cursor ->
                while (cursor.moveToNext()) {
                    if (each(cursor) == false) break
                }
            }
        } catch (e: RuntimeException) {
            // SecurityException, or a provider that refuses the query.
            Log.w(TAG, "could not read the SMS: ${e.javaClass.simpleName}")
        }
    }

    private fun Cursor.toRow() = SmsRow(
        id = getLong(0),
        thread = getLong(1),
        address = getString(2),
        body = getString(3),
        date = getLong(4),
        type = getInt(5),
        read = getInt(6) != 0,
    )

    class SmsError(val code: String) : Exception(code)

    companion object {
        private val ROW_COLUMNS = arrayOf(
            Telephony.Sms._ID,
            Telephony.Sms.THREAD_ID,
            Telephony.Sms.ADDRESS,
            Telephony.Sms.BODY,
            Telephony.Sms.DATE,
            Telephony.Sms.TYPE,
            Telephony.Sms.READ,
        )

        /** Most rows read to find the newest threads, so a phone with years of SMS answers quickly. */
        private const val MAX_SCANNED_ROWS = 20_000

        // The `error` of a failed `sms.sent`.
        const val ERROR_PERMISSION = "permission"
        const val ERROR_NO_SMS = "noSms"
        const val ERROR_FAILED = "failed"
        const val ERROR_NO_SERVICE = "noService"
        const val ERROR_RADIO_OFF = "radioOff"

        fun permissions(): List<String> = listOf(Manifest.permission.READ_SMS, Manifest.permission.SEND_SMS, Manifest.permission.RECEIVE_SMS)

        fun missingPermissions(context: Context): List<String> = permissions().filterNot { context.has(it) }

        /** The `error` for the result code of a sent part; null when it left. */
        fun errorOf(resultCode: Int): String? = when (resultCode) {
            Activity.RESULT_OK -> null
            SmsManager.RESULT_ERROR_NO_SERVICE -> ERROR_NO_SERVICE
            SmsManager.RESULT_ERROR_RADIO_OFF -> ERROR_RADIO_OFF
            else -> ERROR_FAILED
        }

        /** Every contact with a phone number, sorted by name, at most [Packet.MAX_CONTACTS]. Blocking. */
        fun contacts(context: Context): List<Contact> {
            if (!context.has(Manifest.permission.READ_CONTACTS)) return emptyList()
            val byContact = LinkedHashMap<Long, Pair<String, LinkedHashSet<String>>>()
            try {
                context.contentResolver.query(
                    ContactsContract.CommonDataKinds.Phone.CONTENT_URI,
                    arrayOf(
                        ContactsContract.CommonDataKinds.Phone.CONTACT_ID,
                        ContactsContract.CommonDataKinds.Phone.DISPLAY_NAME_PRIMARY,
                        ContactsContract.CommonDataKinds.Phone.NUMBER,
                    ),
                    null,
                    null,
                    "${ContactsContract.CommonDataKinds.Phone.DISPLAY_NAME_PRIMARY} COLLATE NOCASE ASC",
                )?.use { cursor ->
                    while (cursor.moveToNext()) {
                        val number = cursor.getString(2)?.takeIf { it.isNotBlank() } ?: continue
                        val id = cursor.getLong(0)
                        val entry = byContact[id] ?: run {
                            if (byContact.size >= Packet.MAX_CONTACTS) return@run null
                            val name = cursor.getString(1)?.takeIf { it.isNotBlank() } ?: number
                            (name to LinkedHashSet<String>()).also { byContact[id] = it }
                        } ?: continue
                        entry.second += number
                    }
                }
            } catch (e: RuntimeException) {
                Log.w(TAG, "could not read the contacts: ${e.javaClass.simpleName}")
            }
            return byContact.values.map { (name, numbers) -> Contact(name, numbers.toList()) }
        }    }
}
