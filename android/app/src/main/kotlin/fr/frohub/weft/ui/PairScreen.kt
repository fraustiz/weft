package fr.frohub.weft.ui

import android.Manifest
import android.app.Activity
import android.content.ClipboardManager
import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.annotation.StringRes
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialogDefaults
import androidx.compose.material3.BasicAlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import fr.frohub.weft.R
import fr.frohub.weft.core.ConnectionProblem
import fr.frohub.weft.core.Device
import fr.frohub.weft.core.InviteResult
import fr.frohub.weft.core.InviteState
import fr.frohub.weft.core.JoinResult
import fr.frohub.weft.core.JoinState
import fr.frohub.weft.protocol.ControlError
import fr.frohub.weft.ui.scan.QrScanner

/** What a QR scanner composable takes: where to report a code, and a camera failure. */
typealias Scanner = @Composable (onCode: (String) -> Unit, onUnavailable: () -> Unit, modifier: Modifier) -> Unit

/** The CameraX and ZXing scanner. */
val DefaultScanner: Scanner = { onCode, onUnavailable, modifier -> QrScanner(onCode, onUnavailable, modifier) }

/**
 * Pairing: scan the code shown by the computer, or paste it. A phone with no device joins the
 * computer's group (docs/protocol.md section 5, device B). A phone already in a group is asked
 * first: add the computer to its group (the default, « Invitation »), or leave the group and
 * join the computer's. Then the progress, and either success or what went wrong with a way to
 * try again. A successful join is handled by the caller; a successful invitation shows here
 * until [onDone].
 *
 * [deviceCount] is how many devices this phone trusts. [onRetry] forgets the last result.
 */
@Composable
fun PairScreen(
    joinState: JoinState,
    onJoin: (code: String) -> Unit,
    onRetry: () -> Unit,
    onBack: () -> Unit,
    scanner: Scanner = DefaultScanner,
    deviceCount: Int = 0,
    inviteState: InviteState = InviteState.Idle,
    onInvite: (code: String) -> Unit = {},
    onDone: () -> Unit = onBack,
) {
    var code by rememberSaveable { mutableStateOf("") }
    // The code waiting for the choice between adding and joining, and the last code used.
    var choosing by rememberSaveable { mutableStateOf<String?>(null) }
    var lastCode by rememberSaveable { mutableStateOf<String?>(null) }
    // The camera keeps reading the code it is pointed at: a choice cancelled for one code is not
    // asked again for that code unless it is submitted by hand.
    var dismissed by rememberSaveable { mutableStateOf<String?>(null) }
    val joinFailure = (joinState as? JoinState.Done)?.result?.takeIf { it !is JoinResult.Joined }
    val inviteDone = (inviteState as? InviteState.Done)?.result
    // A finished join shows as in progress until the caller moves on.
    val phase = when {
        inviteDone is InviteResult.Added -> Phase.ADDED
        inviteDone != null -> Phase.INVITE_FAILED
        inviteState == InviteState.Inviting -> Phase.INVITING
        joinFailure != null -> Phase.FAILED
        joinState == JoinState.Idle -> Phase.INPUT
        else -> Phase.JOINING
    }
    val join = { pairingCode: String ->
        lastCode = pairingCode
        onJoin(pairingCode)
    }
    val invite = { pairingCode: String ->
        lastCode = pairingCode
        onInvite(pairingCode)
    }
    val submit = { pairingCode: String, scanned: Boolean ->
        val trimmed = pairingCode.trim()
        when {
            deviceCount == 0 -> join(trimmed)
            choosing != null || (scanned && trimmed == dismissed) -> Unit
            else -> {
                choosing = trimmed
            }
        }
    }
    val title = stringResource(if (deviceCount > 0) R.string.action_add_device else R.string.pair_title)
    Scaffold(topBar = { BackTopBar(title, onBack) }) { innerPadding ->
        AnimatedContent(
            targetState = phase,
            transitionSpec = { fadeIn(tween(MotionMedium)) togetherWith fadeOut(tween(MotionShort)) },
            label = "pairPhase",
            modifier = Modifier.padding(innerPadding),
        ) { shown ->
            when (shown) {
                Phase.INPUT -> PairInput(
                    code = code,
                    onCodeChange = { code = it },
                    onScanned = { submit(it, true) },
                    onSubmit = { submit(code, false) },
                    scanner = scanner,
                )
                Phase.JOINING -> Waiting(R.string.joining_title, R.string.joining_body)
                Phase.INVITING -> Waiting(R.string.inviting_title, R.string.inviting_body)
                Phase.FAILED -> {
                    val (failureTitle, body) = joinErrorText(joinFailure ?: JoinResult.Busy)
                    PairFailed(failureTitle, body, onRetry, onBack)
                }
                Phase.INVITE_FAILED -> {
                    val result = inviteDone ?: InviteResult.Busy
                    val (failureTitle, body) = inviteErrorText(result)
                    val retryCode = lastCode
                    // No group to add to: pairing with the computer is what is left.
                    val instead = if (result == InviteResult.NoGroup && retryCode != null) {
                        {
                            onRetry()
                            join(retryCode)
                        }
                    } else {
                        null
                    }
                    PairFailed(failureTitle, body, onRetry, onBack, joinInstead = instead)
                }
                Phase.ADDED -> Added(
                    (inviteDone as? InviteResult.Added)?.device,
                    onDone = onDone,
                    onAddAnother = {
                        code = ""
                        onRetry()
                    },
                )
            }
        }
    }

    choosing?.let { pending ->
        PairChoiceDialog(
            deviceCount = deviceCount,
            onAdd = {
                choosing = null
                invite(pending)
            },
            onJoin = {
                choosing = null
                join(pending)
            },
            onDismiss = {
                choosing = null
                dismissed = pending
            },
        )
    }
}

private enum class Phase { INPUT, JOINING, INVITING, FAILED, INVITE_FAILED, ADDED }

/**
 * Asked when a code is used on a phone that is already in a group: add the computer to that
 * group (the primary action), or move this phone to the computer's group, forgetting its
 * devices (quiet, with what it costs written under it).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PairChoiceDialog(deviceCount: Int, onAdd: () -> Unit, onJoin: () -> Unit, onDismiss: () -> Unit) {
    BasicAlertDialog(onDismissRequest = onDismiss, modifier = Modifier.testTag(CHOICE_DIALOG)) {
        Surface(
            shape = AlertDialogDefaults.shape,
            color = AlertDialogDefaults.containerColor,
            tonalElevation = AlertDialogDefaults.TonalElevation,
        ) {
            Column(Modifier.padding(24.dp)) {
                Text(
                    stringResource(R.string.invite_choice_title),
                    style = MaterialTheme.typography.headlineSmall,
                    modifier = Modifier.semantics { heading() },
                )
                Spacer(Modifier.height(16.dp))
                Text(
                    pluralStringResource(R.plurals.invite_choice_body, deviceCount, deviceCount),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(24.dp))
                Button(
                    onClick = onAdd,
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = 48.dp)
                        .testTag(CHOICE_ADD),
                ) {
                    Text(stringResource(R.string.invite_choice_add), textAlign = TextAlign.Center)
                }
                Spacer(Modifier.height(12.dp))
                OutlinedButton(
                    onClick = onJoin,
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = MaterialTheme.colorScheme.onSurface),
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = 48.dp)
                        .testTag(CHOICE_JOIN),
                ) {
                    Text(stringResource(R.string.invite_choice_join), textAlign = TextAlign.Center)
                }
                Spacer(Modifier.height(8.dp))
                Text(
                    pluralStringResource(R.plurals.invite_choice_join_hint, deviceCount, deviceCount),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(16.dp))
                TextButton(
                    onClick = onDismiss,
                    colors = quietTextButtonColors(),
                    modifier = Modifier.align(Alignment.End),
                ) {
                    Text(stringResource(R.string.action_cancel))
                }
            }
        }
    }
}

@Composable
private fun PairInput(
    code: String,
    onCodeChange: (String) -> Unit,
    onScanned: (String) -> Unit,
    onSubmit: () -> Unit,
    scanner: Scanner,
) {
    val context = LocalContext.current
    var pasteEmpty by remember { mutableStateOf(false) }
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 16.dp, vertical = 8.dp),
    ) {
        CameraBlock(onScanned, scanner)
        Spacer(Modifier.height(32.dp))
        Text(
            stringResource(R.string.paste_title),
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier
                .padding(horizontal = 4.dp)
                .semantics { heading() },
        )
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(
            value = code,
            onValueChange = {
                pasteEmpty = false
                onCodeChange(it)
            },
            label = { Text(stringResource(R.string.code_label)) },
            placeholder = { Text(stringResource(R.string.code_placeholder), style = MonoStyle) },
            textStyle = MaterialTheme.typography.bodyLarge.merge(MonoStyle),
            singleLine = true,
            colors = fieldColors(),
            supportingText = if (pasteEmpty) {
                { Text(stringResource(R.string.paste_nothing)) }
            } else {
                null
            },
            trailingIcon = {
                IconButton(
                    onClick = {
                        val text = readClipboardText(context)
                        pasteEmpty = text == null
                        if (text != null) onCodeChange(text)
                    },
                    modifier = Modifier.testTag(PASTE_BUTTON),
                ) {
                    Icon(painterResource(R.drawable.ic_content_paste), contentDescription = stringResource(R.string.action_paste))
                }
            },
            keyboardOptions = KeyboardOptions(
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
                imeAction = ImeAction.Done,
            ),
            keyboardActions = KeyboardActions(onDone = { if (code.isNotBlank()) onSubmit() }),
            modifier = Modifier
                .fillMaxWidth()
                .testTag(CODE_FIELD),
        )
        Spacer(Modifier.height(24.dp))
        Button(
            onClick = onSubmit,
            enabled = code.isNotBlank(),
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 56.dp)
                .testTag(JOIN_BUTTON),
        ) {
            Text(stringResource(R.string.action_join), style = MaterialTheme.typography.titleMedium)
        }
        Spacer(Modifier.height(16.dp))
    }
}

/** The viewfinder, or what stands in for it until the camera can be used. */
@Composable
private fun CameraBlock(onScanned: (String) -> Unit, scanner: Scanner) {
    val context = LocalContext.current
    val hasCamera = remember { context.packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY) }
    var granted by remember { mutableStateOf(context.hasCameraPermission()) }
    // Denied with "don't ask again" (or twice): only Settings can turn it back on.
    var blocked by rememberSaveable { mutableStateOf(false) }
    var unavailable by rememberSaveable { mutableStateOf(false) }
    val request = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        granted = ok
        blocked = !ok && context.findActivity()?.shouldShowRequestPermissionRationale(Manifest.permission.CAMERA) == false
    }
    // Back from Settings, or a permission changed elsewhere.
    LifecycleResumeEffect(Unit) {
        granted = context.hasCameraPermission()
        if (granted) blocked = false
        onPauseOrDispose {}
    }
    val viewfinderLabel = stringResource(R.string.scan_viewfinder)
    Column {
        Box(
            Modifier
                .fillMaxWidth()
                .aspectRatio(4f / 3f)
                .clip(RoundedCornerShape(16.dp))
                .background(MaterialTheme.colorScheme.surfaceContainerHigh)
                .testTag(CAMERA_BLOCK),
            contentAlignment = Alignment.Center,
        ) {
            when {
                !hasCamera || unavailable -> CameraMessage(stringResource(R.string.camera_unavailable))
                granted -> scanner(
                    onScanned,
                    { unavailable = true },
                    Modifier
                        .fillMaxSize()
                        .semantics { contentDescription = viewfinderLabel },
                )
                blocked -> CameraMessage(stringResource(R.string.camera_denied)) {
                    OutlinedButton(onClick = { context.openAppSettings() }) {
                        Text(stringResource(R.string.action_open_settings))
                    }
                }
                else -> CameraMessage(stringResource(R.string.camera_rationale)) {
                    Button(onClick = { request.launch(Manifest.permission.CAMERA) }) {
                        Text(stringResource(R.string.action_allow_camera))
                    }
                }
            }
        }
        Spacer(Modifier.height(12.dp))
        Text(
            commandText(R.string.scan_hint),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(horizontal = 4.dp),
        )
    }
}

@Composable
private fun CameraMessage(text: String, action: (@Composable () -> Unit)? = null) {
    Column(
        Modifier
            .verticalScroll(rememberScrollState())
            .padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text(
            text,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        action?.invoke()
    }
}

@Composable
private fun Waiting(@StringRes title: Int, @StringRes body: Int) {
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 24.dp, vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        CircularProgressIndicator()
        Spacer(Modifier.height(24.dp))
        Text(
            stringResource(title),
            style = MaterialTheme.typography.titleLarge,
            modifier = Modifier
                .testTag(JOINING)
                .semantics { liveRegion = LiveRegionMode.Polite },
        )
        Spacer(Modifier.height(8.dp))
        Text(
            commandText(body),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun PairFailed(
    @StringRes title: Int,
    @StringRes body: Int,
    onRetry: () -> Unit,
    onCancel: () -> Unit,
    joinInstead: (() -> Unit)? = null,
) {
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 16.dp, vertical = 24.dp),
    ) {
        Row(verticalAlignment = Alignment.Top) {
            // Centred on the first line of the title.
            FirstLine(MaterialTheme.typography.headlineSmall) { StatusPixel(StatusTone.ERROR) }
            Spacer(Modifier.width(12.dp))
            Text(
                stringResource(title),
                style = MaterialTheme.typography.headlineSmall,
                modifier = Modifier
                    .testTag(JOIN_ERROR)
                    .semantics {
                        heading()
                        liveRegion = LiveRegionMode.Polite
                    },
            )
        }
        Spacer(Modifier.height(12.dp))
        Text(
            stringResource(body),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(start = 22.dp),
        )
        Spacer(Modifier.height(32.dp))
        Button(
            onClick = joinInstead ?: onRetry,
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 56.dp)
                .testTag(if (joinInstead != null) JOIN_INSTEAD_BUTTON else RETRY_BUTTON),
        ) {
            Text(
                stringResource(if (joinInstead != null) R.string.action_join_instead else R.string.action_try_again),
                style = MaterialTheme.typography.titleMedium,
            )
        }
        Spacer(Modifier.height(8.dp))
        TextButton(
            onClick = onCancel,
            colors = quietTextButtonColors(),
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp),
        ) {
            Text(stringResource(R.string.action_cancel))
        }
    }
}

/** Title and advice for a failed join. */
fun joinErrorText(result: JoinResult): Pair<Int, Int> = when (result) {
    is JoinResult.Joined, JoinResult.Busy -> R.string.join_busy to R.string.join_busy_body
    JoinResult.InvalidCode -> R.string.join_invalid_code to R.string.join_invalid_code_body
    JoinResult.OwnCode -> R.string.join_own_code to R.string.join_own_code_body
    JoinResult.InsecureRelay -> R.string.join_insecure_relay to R.string.join_insecure_relay_body
    JoinResult.TimedOut -> R.string.join_timed_out to R.string.join_timed_out_body
    is JoinResult.ConnectionFailed -> when (result.problem) {
        ConnectionProblem.UNREACHABLE -> R.string.join_unreachable to R.string.join_unreachable_body
        ConnectionProblem.TIMED_OUT -> R.string.join_relay_timed_out to R.string.join_relay_timed_out_body
        ConnectionProblem.WRONG_RELAY_NAME -> R.string.join_wrong_relay_name to R.string.join_wrong_relay_name_body
        ConnectionProblem.REJECTED -> R.string.join_rejected to R.string.join_rejected_body
        ConnectionProblem.INSECURE -> R.string.join_insecure_relay to R.string.join_insecure_relay_body
        ConnectionProblem.PROTOCOL -> R.string.join_protocol to R.string.join_protocol_body
        ConnectionProblem.LOST -> R.string.join_lost to R.string.join_lost_body
    }
    is JoinResult.Refused -> when (result.error) {
        ControlError.UNKNOWN_PAIRING -> R.string.join_expired to R.string.join_expired_body
        ControlError.NOT_ALLOWED -> R.string.join_not_allowed to R.string.join_not_allowed_body
        ControlError.RATE_LIMITED -> R.string.join_rate_limited to R.string.join_rate_limited_body
        ControlError.GROUP_FULL -> R.string.join_group_full to R.string.join_group_full_body
    }
}

/** Title and advice for a failed invitation; the relay's answers read as for a join. */
fun inviteErrorText(result: InviteResult): Pair<Int, Int> = when (result) {
    is InviteResult.Added, InviteResult.Busy -> R.string.join_busy to R.string.join_busy_body
    InviteResult.InvalidCode -> R.string.join_invalid_code to R.string.join_invalid_code_body
    InviteResult.OwnCode -> R.string.join_own_code to R.string.join_own_code_body
    InviteResult.NoGroup -> R.string.invite_no_group to R.string.invite_no_group_body
    InviteResult.OtherRelay -> R.string.invite_other_relay to R.string.invite_other_relay_body
    InviteResult.GroupFull -> R.string.join_group_full to R.string.invite_group_full_body
    InviteResult.NoAnswer -> R.string.invite_no_answer to R.string.invite_no_answer_body
    is InviteResult.ConnectionFailed -> joinErrorText(JoinResult.ConnectionFailed(result.reason, result.problem))
    is InviteResult.Refused -> when (result.error) {
        ControlError.GROUP_FULL -> R.string.join_group_full to R.string.invite_group_full_body
        else -> joinErrorText(JoinResult.Refused(result.error))
    }
}

/** The computer is in the group: say so, by its name, then back to the devices. */
@Composable
private fun Added(device: Device?, onDone: () -> Unit, onAddAnother: () -> Unit) {
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 16.dp, vertical = 24.dp),
    ) {
        Row(verticalAlignment = Alignment.Top) {
            // Centred on the first line of the title.
            FirstLine(MaterialTheme.typography.headlineSmall) { StatusPixel(StatusTone.GOOD) }
            Spacer(Modifier.width(12.dp))
            Text(
                stringResource(R.string.invite_added_title, device?.label.orEmpty()),
                style = MaterialTheme.typography.headlineSmall,
                modifier = Modifier
                    .testTag(INVITE_ADDED)
                    .semantics {
                        heading()
                        liveRegion = LiveRegionMode.Polite
                    },
            )
        }
        Spacer(Modifier.height(12.dp))
        Text(
            stringResource(R.string.invite_added_body),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(start = 22.dp),
        )
        if (device?.name != null) {
            Spacer(Modifier.height(4.dp))
            Text(
                device.shortId,
                style = MaterialTheme.typography.bodySmall.merge(MonoStyle),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(start = 22.dp),
            )
        }
        Spacer(Modifier.height(32.dp))
        Button(
            onClick = onDone,
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 56.dp)
                .testTag(DONE_BUTTON),
        ) {
            Text(stringResource(R.string.action_done), style = MaterialTheme.typography.titleMedium)
        }
        Spacer(Modifier.height(8.dp))
        TextButton(
            onClick = onAddAnother,
            colors = quietTextButtonColors(),
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp),
        ) {
            Text(stringResource(R.string.action_add_another))
        }
    }
}

/** The text on the clipboard, trimmed, or null. Reading it needs focus, which a tap gives. */
private fun readClipboardText(context: Context): String? {
    val clip = context.getSystemService(ClipboardManager::class.java)?.primaryClip ?: return null
    if (clip.itemCount == 0) return null
    return clip.getItemAt(0).coerceToText(context)?.toString()?.trim()?.ifEmpty { null }
}

private fun Context.hasCameraPermission() =
    checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED

private fun Context.openAppSettings() {
    startActivity(
        Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", packageName, null))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
    )
}

private tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}

/** Test tags. */
const val CODE_FIELD = "code_field"
const val PASTE_BUTTON = "paste_button"
const val JOIN_BUTTON = "join_button"
const val CAMERA_BLOCK = "camera_block"
const val JOINING = "joining"
const val JOIN_ERROR = "join_error"
const val RETRY_BUTTON = "retry_button"
const val JOIN_INSTEAD_BUTTON = "join_instead_button"
const val CHOICE_DIALOG = "choice_dialog"
const val CHOICE_ADD = "choice_add"
const val CHOICE_JOIN = "choice_join"
const val INVITE_ADDED = "invite_added"
const val DONE_BUTTON = "done_button"

@Preview
@Composable
private fun PairErrorPreview() {
    WeftTheme(darkTheme = false) {
        PairScreen(JoinState.Done(JoinResult.Refused(ControlError.UNKNOWN_PAIRING)), { _ -> }, {}, {})
    }
}

@Preview
@Composable
private fun PairChoicePreview() {
    WeftTheme(darkTheme = false) {
        PairChoiceDialog(deviceCount = 2, onAdd = {}, onJoin = {}, onDismiss = {})
    }
}

@Preview
@Composable
private fun InviteAddedPreview() {
    WeftTheme(darkTheme = true) {
        PairScreen(
            JoinState.Idle, { _ -> }, {}, {},
            deviceCount = 2,
            inviteState = InviteState.Done(InviteResult.Added(Device("3f9a0c12d4e5f60718293a4b5c6d7e8f", "Studio iMac"))),
        )
    }
}
