package io.github.lawkitt.mdoc.document

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import io.github.lawkitt.mdoc.R

/**
 * A locked DOCX's password, as on iOS. The password stays in this field
 * until Unlock hands it to the one import that uses it; the field is
 * emptied then, and after a wrong one. Not saved with the screen's state.
 * At large font scales the notices column it sits in scrolls; the field and
 * Unlock are brought into view above the keyboard whenever the field has
 * focus.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun PasswordCard(prompt: PasswordPrompt, onUnlock: (String) -> Unit, modifier: Modifier = Modifier) {
    var password by remember { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    val entry = remember { BringIntoViewRequester() }
    var focused by remember { mutableStateOf(false) }
    val keyboard = WindowInsets.ime.getBottom(LocalDensity.current)
    LaunchedEffect(focused, keyboard, prompt) {
        if (focused) entry.bringIntoView()
    }
    val submit = {
        if (password.isNotEmpty()) {
            val given = password
            password = ""
            onUnlock(given)
        }
    }
    LaunchedEffect(prompt) { focus.requestFocus() }
    Card(modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(
                stringResource(R.string.password_title),
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.semantics { heading() },
            )
            Text(stringResource(R.string.password_body), style = MaterialTheme.typography.bodyMedium)
            if (prompt == PasswordPrompt.Wrong) {
                Text(
                    stringResource(R.string.password_wrong),
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.testTag("password-wrong"),
                )
            }
            Column(Modifier.bringIntoViewRequester(entry), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            OutlinedTextField(
                value = password,
                onValueChange = { password = it },
                label = { Text(stringResource(R.string.password_label)) },
                singleLine = true,
                visualTransformation = PasswordVisualTransformation(),
                keyboardOptions = KeyboardOptions(
                    keyboardType = KeyboardType.Password,
                    autoCorrectEnabled = false,
                    imeAction = ImeAction.Go,
                ),
                keyboardActions = KeyboardActions(onGo = { submit() }),
                modifier = Modifier
                    .fillMaxWidth()
                    .focusRequester(focus)
                    .onFocusChanged { focused = it.isFocused }
                    .testTag("document-password"),
            )
            Button(onClick = submit, enabled = password.isNotEmpty(), modifier = Modifier.testTag("unlock-document")) {
                Text(stringResource(R.string.unlock))
            }
            }
        }
    }
}
