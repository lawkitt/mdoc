package io.github.lawkitt.mdoc.restore

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.lawkitt.mdoc.MdocApplication
import io.github.lawkitt.mdoc.R
import io.github.lawkitt.mdoc.core.CaseId
import io.github.lawkitt.mdoc.core.Documents
import io.github.lawkitt.mdoc.core.LibrarySnapshot
import io.github.lawkitt.mdoc.core.RestoreResult
import io.github.lawkitt.mdoc.document.copy
import io.github.lawkitt.mdoc.ui.MdocTheme
import io.github.lawkitt.mdoc.ui.RestoredText
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.mdoc_ffi.markdownStyling

/**
 * Restore with mdoc (ADR 0085, 0089): an AI app shares its reply here as
 * text; mdoc restores it with the case of the most recently shared document,
 * with a picker for another case and never mixing two, then offers Copy,
 * Share and Save to document. It runs in mdoc's own process, so it opens the
 * same library and case key as the app and needs no model.
 */
class RestoreActivity : ComponentActivity() {
    /** The reply being restored; a new share replaces it (the activity is single-task). */
    private var reply by mutableStateOf<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        reply = replyOf(intent)
        val documents = runCatching { (application as MdocApplication).core.documents }.getOrNull()
        setContent {
            MdocTheme {
                // Keyed on the reply: a new one starts afresh, never showing the last.
                key(reply) { RestoreScreen(reply, documents, onDone = ::finish) }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        reply = replyOf(intent)
    }

    private fun replyOf(intent: Intent?): String? =
        intent?.takeIf { it.action == Intent.ACTION_SEND }?.getCharSequenceExtra(Intent.EXTRA_TEXT)?.toString()?.takeIf { it.isNotBlank() }
}

private data class Restored(val result: RestoreResult?, val error: String?)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun RestoreScreen(reply: String?, documents: Documents?, onDone: () -> Unit) {
    var library by remember { mutableStateOf<LibrarySnapshot?>(null) }
    var case by remember { mutableStateOf<CaseId?>(null) }
    var restored by remember { mutableStateOf<Restored?>(null) }
    var notice by remember { mutableStateOf<Int?>(null) }
    LaunchedEffect(documents) {
        if (documents == null) return@LaunchedEffect
        val snapshot = withContext(Dispatchers.IO) { documents.snapshot() }
        library = snapshot
        case = snapshot.currentCase ?: snapshot.cases.firstOrNull()?.id
    }
    LaunchedEffect(case) {
        val chosen = case ?: return@LaunchedEffect
        if (reply == null || documents == null) return@LaunchedEffect
        restored = withContext(Dispatchers.IO) {
            runCatching { documents.restore(reply, chosen) }
                .fold({ Restored(it, null) }, { Restored(null, it.message) })
        }
        notice = null
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.restore_with_mdoc)) },
                actions = { TextButton(onDone) { Text(stringResource(R.string.done)) } },
            )
        },
    ) { padding ->
        val inner = Modifier.padding(padding).fillMaxSize()
        val snapshot = library
        when {
            reply == null || documents == null -> Unavailable(R.string.nothing_to_restore, R.string.share_reply_as_text, inner)
            snapshot == null -> Box(inner)
            snapshot.cases.isEmpty() -> Unavailable(R.string.no_cases_yet, R.string.share_document_first, inner)
            else -> Column(inner) {
                CasePicker(snapshot, case) {
                    // Cleared at once: the last case's names never show under the new one.
                    restored = null
                    case = it
                }
                val result = restored?.result
                if (result != null) {
                    // A tablet keeps a reading width, as the document does.
                    Column(
                        Modifier.weight(1f).fillMaxWidth().wrapContentWidth().widthIn(max = 720.dp)
                            .verticalScroll(rememberScrollState()).padding(16.dp),
                    ) {
                        Text(
                            listOfNotNull(
                                pluralStringResource(R.plurals.values_restored, result.restoredCount, result.restoredCount),
                                if (result.unresolvedCount > 0) stringResource(R.string.unrecognised, result.unresolvedCount) else null,
                            ).joinToString(" · "),
                            style = MaterialTheme.typography.labelLarge,
                        )
                        RestoredText(result, markdownStyling(result.text), Modifier.fillMaxWidth().padding(top = 8.dp))
                    }
                    Actions(
                        result = result,
                        // The case's newest saved document: an import not yet read has nowhere to keep it.
                        saveTarget = snapshot.documents.firstOrNull { it.caseId == case && it.saved },
                        notice = notice,
                        onSaved = { notice = R.string.saved },
                        onCopied = { notice = R.string.copied_reply },
                        onFailed = { notice = R.string.not_saved },
                        save = { target -> documents.saveReply(reply, target) },
                    )
                } else if (restored?.error != null) {
                    Unavailable(R.string.not_restored, null, Modifier.weight(1f), restored?.error)
                }
            }
        }
    }
}

@Composable
private fun CasePicker(library: LibrarySnapshot, selected: CaseId?, onSelect: (CaseId) -> Unit) {
    var open by remember { mutableStateOf(false) }
    val name = library.case(selected)?.name.orEmpty()
    Surface(tonalElevation = 1.dp, modifier = Modifier.fillMaxWidth()) {
        Row(Modifier.padding(16.dp, 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(stringResource(R.string.case_label), style = MaterialTheme.typography.labelLarge, modifier = Modifier.weight(1f))
            Box {
                // A picker, and it looks like one.
                TextButton({ open = true }) {
                    Text(name, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Icon(Icons.Default.ArrowDropDown, contentDescription = null)
                }
                DropdownMenu(open, { open = false }) {
                    library.cases.forEach { option ->
                        DropdownMenuItem(text = { Text(option.name) }, onClick = {
                            open = false
                            onSelect(option.id)
                        })
                    }
                }
            }
        }
    }
}

@Composable
private fun Actions(
    result: RestoreResult,
    saveTarget: io.github.lawkitt.mdoc.core.DocumentSummary?,
    notice: Int?,
    onSaved: () -> Unit,
    onCopied: () -> Unit,
    onFailed: () -> Unit,
    save: (io.github.lawkitt.mdoc.core.DocumentId) -> Unit,
) {
    val context = androidx.compose.ui.platform.LocalContext.current
    Surface(tonalElevation = 3.dp) {
        Column(
            Modifier.fillMaxWidth().navigationBarsPadding().wrapContentWidth().widthIn(max = 720.dp).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            notice?.let { Text(stringResource(it), style = MaterialTheme.typography.labelLarge) }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                // Copy leads: most replies go back into a message (ADR 0087).
                Button({
                    // The restored reply holds originals: marked sensitive (lawkitt/mdoc#321).
                    copy(context, result.text, sensitive = true)
                    onCopied()
                }, Modifier.weight(1f)) { Text(stringResource(R.string.copy)) }
                FilledTonalButton({
                    val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, result.text)
                    context.startActivity(Intent.createChooser(send, null))
                }, Modifier.weight(1f)) { Text(stringResource(R.string.share)) }
            }
            if (saveTarget != null) {
                OutlinedButton({
                    runCatching { save(saveTarget.id) }.onSuccess { onSaved() }.onFailure { onFailed() }
                }, Modifier.fillMaxWidth()) { Text(stringResource(R.string.save_to_document)) }
            }
        }
    }
}

@Composable
private fun Unavailable(title: Int, body: Int?, modifier: Modifier = Modifier, detail: String? = null) {
    Column(
        modifier.padding(32.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(stringResource(title), style = MaterialTheme.typography.titleLarge, modifier = Modifier.semantics { heading() })
        (detail ?: body?.let { stringResource(it) })?.let {
            Text(it, style = MaterialTheme.typography.bodyMedium, textAlign = TextAlign.Center, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}
