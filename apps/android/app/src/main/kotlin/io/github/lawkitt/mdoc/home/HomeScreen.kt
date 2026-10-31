package io.github.lawkitt.mdoc.home

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.lawkitt.mdoc.R
import io.github.lawkitt.mdoc.core.CaseChoice
import io.github.lawkitt.mdoc.core.CaseId
import io.github.lawkitt.mdoc.core.CaseSummary
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.DocumentSummary
import io.github.lawkitt.mdoc.ui.Palette

/** The four ways in, in menu order: the camera last, furthest from the button (ADR 0088). */
enum class AddOption { Paste, Files, Photos, Scan }

/**
 * Home: a flat list of documents, newest first, each with its case's dot and
 * a status line (ADR 0087), or how to add the first one. A library that could
 * not be opened says so rather than looking empty.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(
    state: HomeState,
    onOpen: (DocumentId) -> Unit,
    onAdd: (AddOption) -> Unit,
    onRetry: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var menu by rememberSaveable { mutableStateOf(false) }
    Scaffold(
        modifier = modifier,
        topBar = { TopAppBar(title = { Text(stringResource(R.string.app_name)) }) },
        floatingActionButton = {
            Box {
                ExtendedFloatingActionButton(
                    onClick = { menu = true },
                    icon = { Icon(Icons.Default.Add, contentDescription = null) },
                    text = { Text(stringResource(R.string.add)) },
                )
                // Opens upward from the button: Scan, the camera, ends furthest away.
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    AddOption.entries.reversed().forEach { option ->
                        DropdownMenuItem(
                            text = { Text(stringResource(option.label)) },
                            onClick = {
                                menu = false
                                onAdd(option)
                            },
                        )
                    }
                }
            }
        },
    ) { padding ->
        val inner = Modifier.padding(padding)
        when {
            state.error != null -> CoreFailed(state.error, onRetry, inner)
            !state.loaded -> Box(inner.fillMaxSize())
            state.documents.isEmpty() -> EmptyHome(state.canFindNames, inner)
            else -> LazyColumn(inner) {
                items(state.documents, key = { it.id.raw }) { document ->
                    DocumentRow(document, state.cases.firstOrNull { it.id == document.caseId }, state.canFindNames) {
                        onOpen(document.id)
                    }
                }
            }
        }
    }
}

private val AddOption.label: Int
    get() = when (this) {
        AddOption.Paste -> R.string.paste
        AddOption.Files -> R.string.files
        AddOption.Photos -> R.string.photos
        AddOption.Scan -> R.string.scan
    }

@Composable
private fun DocumentRow(document: DocumentSummary, case: CaseSummary?, canFindNames: Boolean, onClick: () -> Unit) {
    val status = when (val line = HomeViewModel.status(document, canFindNames)) {
        StatusLine.NotRecognized -> stringResource(R.string.status_not_recognized)
        StatusLine.ImportNotFinished -> stringResource(R.string.status_import_not_finished)
        StatusLine.ReadyToFind -> stringResource(R.string.status_ready_to_find)
        StatusLine.ReadyToMark -> stringResource(R.string.status_ready_to_mark)
        is StatusLine.ToReview -> stringResource(R.string.status_to_review, line.count)
        StatusLine.ReadyToShare -> stringResource(R.string.status_ready_to_share)
        StatusLine.NothingFound -> stringResource(R.string.status_nothing_found)
    }
    // The case's name only when it differs from the title; the dot is the row's one colour.
    val caseName = case?.name?.takeIf { it != document.title }
    ListItem(
        modifier = Modifier.clickable(onClick = onClick),
        leadingContent = case?.let {
            {
                Box(Modifier.size(10.dp).background(Color(Palette.case(it.colour)), CircleShape))
            }
        },
        headlineContent = { Text(document.title) },
        supportingContent = {
            Text(listOfNotNull(caseName?.let { stringResource(R.string.case_named, it) }, status).joinToString(" · "))
        },
    )
}

@Composable
private fun CoreFailed(error: String, onRetry: () -> Unit, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier.fillMaxSize().padding(32.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(stringResource(R.string.core_failed), style = MaterialTheme.typography.titleLarge, modifier = Modifier.semantics { heading() })
        Text(error, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center)
        Button(onClick = onRetry) { Text(stringResource(R.string.try_again)) }
    }
}

@Composable
private fun EmptyHome(canFindNames: Boolean, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier.fillMaxSize().padding(32.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(stringResource(R.string.home_empty_title), style = MaterialTheme.typography.titleLarge, modifier = Modifier.semantics { heading() })
        Text(
            stringResource(if (canFindNames) R.string.home_empty_body else R.string.home_empty_body_below_floor),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

/**
 * Adding a document: pasted text, a title and a case in one sheet (ADR 0087);
 * a new case unless an existing one is chosen (ADR 0085).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AddSheet(
    adding: PendingAdd,
    cases: List<CaseSummary>,
    onAdd: (text: String?, title: String, case: CaseChoice) -> Unit,
    onDismiss: () -> Unit,
) {
    val pasting = adding is PendingAdd.Text
    var text by rememberSaveable { mutableStateOf("") }
    var title by rememberSaveable { mutableStateOf(adding.suggestedTitle) }
    var case by rememberSaveable { mutableStateOf<String?>(null) }
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(24.dp, 0.dp, 24.dp, 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(
                stringResource(if (pasting) R.string.paste_text else R.string.add_document),
                style = MaterialTheme.typography.titleLarge,
                modifier = Modifier.semantics { heading() },
            )
            if (pasting) {
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    label = { Text(stringResource(R.string.paste_placeholder)) },
                    modifier = Modifier.fillMaxWidth().height(200.dp),
                )
            }
            OutlinedTextField(
                value = title,
                onValueChange = { title = it },
                label = { Text(stringResource(R.string.title)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Text(stringResource(R.string.case_label), style = MaterialTheme.typography.titleSmall)
            CaseOption(stringResource(R.string.new_case), null, case == null) { case = null }
            cases.forEach { option -> CaseOption(option.name, option, case == option.id.raw) { case = option.id.raw } }
            Text(
                stringResource(R.string.case_explanation),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Button(
                onClick = {
                    val name = title.trim().ifEmpty { adding.suggestedTitle }
                    onAdd(if (pasting) text else null, name, case?.let { CaseChoice.Existing(CaseId(it)) } ?: CaseChoice.New(name))
                },
                enabled = !pasting || text.isNotBlank(),
                modifier = Modifier.fillMaxWidth(),
            ) { Text(stringResource(R.string.add)) }
        }
    }
}

@Composable
private fun CaseOption(label: String, case: CaseSummary?, selected: Boolean, onClick: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth().clickable(onClick = onClick)) {
        RadioButton(selected = selected, onClick = onClick)
        case?.let { Box(Modifier.size(10.dp).background(Color(Palette.case(it.colour)), CircleShape)) }
        Text(label, modifier = Modifier.padding(start = 8.dp), style = MaterialTheme.typography.bodyLarge)
    }
}
