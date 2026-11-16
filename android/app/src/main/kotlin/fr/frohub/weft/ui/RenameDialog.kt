package fr.frohub.weft.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import fr.frohub.weft.R
import fr.frohub.weft.core.NameProblem
import fr.frohub.weft.core.RenameResult
import fr.frohub.weft.core.deviceNameProblem
import kotlinx.coroutines.launch

/**
 * Renames this phone. The name is checked as it is typed, with the protocol's rules (1 to 64
 * bytes, no control characters) on the trimmed text. Saving sends it to every other device;
 * if the relay cannot be reached, the dialog says so and offers to send it again.
 */
@Composable
fun RenameDialog(current: String, onRename: suspend (String) -> RenameResult, onDismiss: () -> Unit) {
    var field by remember { mutableStateOf(TextFieldValue(current, TextRange(current.length))) }
    var working by remember { mutableStateOf(false) }
    var offline by rememberSaveable { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val focus = remember { FocusRequester() }
    val name = field.text.trim()
    val problem = deviceNameProblem(name)
    val save = {
        if (problem == null && !working) {
            working = true
            scope.launch {
                val result = onRename(name)
                working = false
                when (result) {
                    is RenameResult.Sent -> onDismiss()
                    RenameResult.NotSent -> offline = true
                    // Checked above with the same rules; only a race could get here.
                    is RenameResult.Invalid -> Unit
                }
            }
        }
    }
    LaunchedEffect(Unit) { focus.requestFocus() }
    AlertDialog(
        onDismissRequest = { if (!working) onDismiss() },
        title = { Text(stringResource(R.string.rename_title)) },
        text = {
            Column {
                Text(
                    stringResource(if (offline) R.string.rename_offline_body else R.string.rename_body),
                    modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                )
                Spacer(Modifier.height(16.dp))
                OutlinedTextField(
                    value = field,
                    onValueChange = {
                        if (it.text != field.text) offline = false
                        field = it
                    },
                    label = { Text(stringResource(R.string.device_name)) },
                    singleLine = true,
                    isError = problem != null,
                    supportingText = if (problem != null) {
                        { Text(stringResource(problem.message)) }
                    } else {
                        null
                    },
                    enabled = !working,
                    colors = fieldColors(),
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { save() }),
                    modifier = Modifier
                        .fillMaxWidth()
                        .focusRequester(focus)
                        .testTag(RENAME_FIELD),
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = { save() },
                enabled = problem == null && !working,
                colors = quietTextButtonColors(),
                modifier = Modifier.testTag(RENAME_SAVE),
            ) {
                if (working) {
                    val label = stringResource(R.string.working)
                    CircularProgressIndicator(
                        strokeWidth = 2.dp,
                        modifier = Modifier
                            .size(20.dp)
                            .semantics { contentDescription = label },
                    )
                } else {
                    Text(stringResource(if (offline) R.string.action_try_again else R.string.rename_save))
                }
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss, enabled = !working, colors = quietTextButtonColors()) {
                Text(stringResource(if (offline) R.string.action_close else R.string.action_cancel))
            }
        },
        modifier = Modifier.testTag(RENAME_DIALOG),
    )
}

private val NameProblem.message: Int
    get() = when (this) {
        NameProblem.EMPTY -> R.string.name_empty
        NameProblem.TOO_LONG -> R.string.name_too_long
        NameProblem.INVALID_CHARACTER -> R.string.name_invalid_character
    }

/** Test tags. */
const val RENAME_DIALOG = "rename_dialog"
const val RENAME_FIELD = "rename_field"
const val RENAME_SAVE = "rename_save"
const val RENAME_MENU_ITEM = "rename_menu_item"

@Preview
@Composable
private fun RenamePreview() {
    WeftTheme(darkTheme = false) {
        RenameDialog("Pixel 8", { RenameResult.Sent(1) }, {})
    }
}
