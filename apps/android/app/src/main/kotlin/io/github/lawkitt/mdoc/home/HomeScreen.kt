package io.github.lawkitt.mdoc.home

import android.content.ClipDescription
import android.content.ClipboardManager
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Person
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.lawkitt.mdoc.R
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.DocumentSummary
import io.github.lawkitt.mdoc.core.ProjectId
import io.github.lawkitt.mdoc.core.ProjectSummary
import io.github.lawkitt.mdoc.ui.Palette

/** The four ways in, in menu order: the camera last, furthest from the button. */
enum class AddOption { Paste, Files, Photos, Scan }

/** All Documents' search shows from this many documents (iOS `searchThreshold`, ADR 0099). */
const val SEARCH_THRESHOLD = 8

/**
 * Home (#1091 Decision 33; iOS's Start): the setup card while models are
 * missing, the document to go back to, and what mdoc does. Lists live in
 * the sidebar and its pages. A library that could not be opened says so
 * rather than looking empty.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(
    state: HomeState,
    onOpen: (DocumentId) -> Unit,
    onAdd: (AddOption) -> Unit,
    onRetry: () -> Unit,
    modifier: Modifier = Modifier,
    setupCard: (@Composable () -> Unit)? = null,
    onSettings: () -> Unit = {},
    onMenu: (() -> Unit)? = null,
    snackbar: SnackbarHostState = remember { SnackbarHostState() },
) {
    Scaffold(
        modifier = modifier,
        snackbarHost = { SnackbarHost(snackbar) },
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.app_name)) },
                navigationIcon = { onMenu?.let { MenuButton(it) } },
                actions = {
                    IconButton(onSettings) { Icon(Icons.Default.Settings, contentDescription = stringResource(R.string.settings)) }
                },
            )
        },
        floatingActionButton = { AddButton(onAdd) },
    ) { padding ->
        // A tablet keeps it to a readable width, centred.
        val inner = Modifier.padding(padding).fillMaxWidth().wrapContentWidth().widthIn(max = 720.dp)
        when {
            state.error != null -> CoreFailed(state.error, onRetry, inner)
            !state.loaded -> Box(inner.fillMaxSize())
            // Scrolls, so the setup card's buttons stay reachable on a short screen or at large text.
            else -> Column(inner.verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                setupCard?.invoke()
                state.continueDocument?.let { ContinueCard(it) { onOpen(it.id) } }
                EmptyHome(state.canFindNames, empty = state.listed.isEmpty(), Modifier.heightIn(min = 320.dp))
            }
        }
    }
}

/** The sidebar's button, on each page's bar while the sidebar is a drawer. */
@Composable
fun MenuButton(onClick: () -> Unit) {
    IconButton(onClick) { Icon(Icons.Default.Menu, contentDescription = stringResource(R.string.show_sidebar)) }
}

/** Add: the four ways in, from the page's floating button. What it adds lands where the page says. */
@Composable
fun AddButton(onAdd: (AddOption) -> Unit) {
    var menu by rememberSaveable { mutableStateOf(false) }
    Box {
        val addLabel = stringResource(R.string.add)
        ExtendedFloatingActionButton(
            onClick = { menu = true },
            // The button's own label: its text alone does not reach TalkBack.
            modifier = Modifier.semantics { contentDescription = addLabel },
            icon = { Icon(Icons.Default.Add, contentDescription = null) },
            text = { Text(stringResource(R.string.add)) },
        )
        // Opens upward from the button: Scan, the camera, ends furthest away.
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            AddOption.entries.reversed().forEach { option ->
                DropdownMenuItem(
                    text = { Text(stringResource(option.label)) },
                    leadingIcon = { Icon(painterResource(option.icon), contentDescription = null) },
                    onClick = {
                        menu = false
                        onAdd(option)
                    },
                )
            }
        }
    }
}

/** "Continue · Services agreement · 3 left". */
@Composable
private fun ContinueCard(document: DocumentSummary, onOpen: () -> Unit) {
    val spoken = stringResource(
        R.string.continue_spoken,
        document.title,
        pluralStringResource(R.plurals.names_left_spoken, document.pendingCount, document.pendingCount),
    )
    Card(onClick = onOpen, modifier = Modifier.fillMaxWidth().clearAndSetSemantics { contentDescription = spoken }) {
        Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Icon(Icons.Outlined.Info, contentDescription = null)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(stringResource(R.string.continue_label), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(document.title, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.Medium, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Text(stringResource(R.string.names_left, document.pendingCount), style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
            }
            Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null)
        }
    }
}

/**
 * All Documents, or Unfiled: documents newest first, each with its
 * Project's dot (where the list mixes Projects) and a status line.
 * Unfiled's Select picks several to delete at once (#1091 Decision 5),
 * also entered by a long press; their snackbar offers Undo.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DocumentListScreen(
    title: String,
    documents: List<DocumentSummary>,
    state: HomeState,
    onOpen: (DocumentId) -> Unit,
    onAdd: (AddOption) -> Unit,
    onDelete: (List<DocumentId>) -> Unit,
    modifier: Modifier = Modifier,
    showsProject: Boolean = true,
    selectable: Boolean = false,
    searchable: Boolean = false,
    emptyTitle: Int = R.string.home_empty_title,
    emptyBody: Int? = null,
    onMenu: (() -> Unit)? = null,
    snackbar: SnackbarHostState = remember { SnackbarHostState() },
    onMove: ((DocumentId, MoveCommand) -> Unit)? = null,
) {
    var selecting by rememberSaveable { mutableStateOf(false) }
    var selected by rememberSaveable { mutableStateOf(listOf<String>()) }
    var query by rememberSaveable { mutableStateOf("") }
    // A document deleted or moved elsewhere leaves the selection.
    val ids = documents.map { it.id.raw }
    LaunchedEffect(ids) { selected = selected.filter { it in ids } }
    val shown = if (searchable && query.isNotBlank()) {
        val found = state.search(query).first.map { it.id }.toSet()
        documents.filter { it.id in found }
    } else {
        documents
    }
    fun endSelecting() {
        selecting = false
        selected = emptyList()
    }
    Scaffold(
        modifier = modifier,
        snackbarHost = { SnackbarHost(snackbar) },
        topBar = {
            if (selecting) {
                // Material's contextual bar while documents are picked.
                TopAppBar(
                    title = { Text(pluralStringResource(R.plurals.selected_count, selected.size, selected.size)) },
                    navigationIcon = {
                        IconButton(::endSelecting) { Icon(Icons.Default.Close, contentDescription = stringResource(R.string.close)) }
                    },
                    actions = {
                        val delete = if (selected.isEmpty()) {
                            stringResource(R.string.delete)
                        } else {
                            pluralStringResource(R.plurals.delete_count, selected.size, selected.size)
                        }
                        TextButton(
                            onClick = {
                                onDelete(documents.map { it.id }.filter { it.raw in selected })
                                endSelecting()
                            },
                            enabled = selected.isNotEmpty(),
                        ) {
                            Icon(Icons.Default.Delete, contentDescription = null, modifier = Modifier.size(18.dp))
                            Spacer(Modifier.width(8.dp))
                            Text(delete)
                        }
                    },
                )
            } else {
                TopAppBar(
                    title = { Text(title) },
                    navigationIcon = { onMenu?.let { MenuButton(it) } },
                    actions = {
                        if (selectable && documents.isNotEmpty()) {
                            TextButton(onClick = { selecting = true }) { Text(stringResource(R.string.select)) }
                        }
                    },
                )
            }
        },
        floatingActionButton = { if (!selecting) AddButton(onAdd) },
    ) { padding ->
        val inner = Modifier.padding(padding).fillMaxWidth().wrapContentWidth().widthIn(max = 720.dp)
        when {
            !state.loaded -> Box(inner.fillMaxSize())
            documents.isEmpty() -> Column(inner.verticalScroll(rememberScrollState())) {
                EmptyState(stringResource(emptyTitle), emptyBody?.let { stringResource(it) }, R.drawable.ic_find_in_page)
            }
            else -> LazyColumn(inner) {
                if (searchable && (documents.size >= SEARCH_THRESHOLD || query.isNotEmpty())) {
                    item(key = "search") {
                        OutlinedTextField(
                            value = query,
                            onValueChange = { query = it },
                            placeholder = { Text(stringResource(R.string.search_documents_and_projects)) },
                            leadingIcon = { Icon(Icons.Outlined.Search, contentDescription = null) },
                            singleLine = true,
                            modifier = Modifier.fillMaxWidth().padding(16.dp, 8.dp),
                        )
                    }
                }
                items(shown, key = { it.id.raw }) { document ->
                    if (selecting) {
                        val isSelected = document.id.raw in selected
                        fun toggle() {
                            selected = if (isSelected) selected - document.id.raw else selected + document.id.raw
                        }
                        DocumentRow(
                            document, null, state.canFindNames,
                            leading = { Checkbox(checked = isSelected, onCheckedChange = { toggle() }) },
                            onClick = ::toggle,
                        )
                    } else {
                        DocumentRow(
                            document,
                            if (showsProject) state.project(document.projectId) else null,
                            state.canFindNames,
                            onDelete = { onDelete(listOf(document.id)) },
                            onSelect = if (selectable) {
                                {
                                    selecting = true
                                    selected = listOf(document.id.raw)
                                }
                            } else {
                                null
                            },
                            moves = documentMoves(document, state, onMove),
                            onMove = { onMove?.invoke(document.id, it) },
                        ) {
                            onOpen(document.id)
                        }
                    }
                }
            }
        }
    }
}

/** iOS's symbols for the same four, as Material Symbols. */
private val AddOption.icon: Int
    get() = when (this) {
        AddOption.Paste -> R.drawable.ic_content_paste
        AddOption.Files -> R.drawable.ic_folder_open
        AddOption.Photos -> R.drawable.ic_photo_library
        AddOption.Scan -> R.drawable.ic_document_scanner
    }

private val AddOption.label: Int
    get() = when (this) {
        AddOption.Paste -> R.string.paste
        AddOption.Files -> R.string.files
        AddOption.Photos -> R.string.photos
        AddOption.Scan -> R.string.scan
    }

/** The moves a listed document's menu offers, when its list moves documents. */
fun documentMoves(document: DocumentSummary, state: HomeState, onMove: Any?): List<MoveCommand> =
    if (onMove == null) emptyList() else HomeViewModel.moveCommands(document.projectId, document.vaultId != null, state.projects)

/** A move command's menu label. */
@Composable
fun MoveCommand.label(): String = stringResource(
    when (this) {
        MoveCommand.ToProject -> R.string.move_to_project_ellipsis
        MoveCommand.ToNewProject -> R.string.move_to_new_project_ellipsis
        MoveCommand.ToUnfiled -> R.string.move_to_unfiled
    },
)

/** A document menu's move commands, in iOS's order: a Project, a new one, then Unfiled. */
@Composable
fun MoveMenuItems(commands: List<MoveCommand>, onMove: (MoveCommand) -> Unit) {
    commands.forEach { command ->
        DropdownMenuItem(text = { Text(command.label()) }, onClick = { onMove(command) })
    }
}

/**
 * A document's row in a list: its Project's dot where the list mixes
 * Projects, its title and status. A long press offers Select in Unfiled,
 * the moves (#1091 Decision 9) and Delete (iOS's swipe); TalkBack has
 * them as actions.
 */
@Composable
fun DocumentRow(
    document: DocumentSummary,
    project: ProjectSummary?,
    canFindNames: Boolean,
    onDelete: (() -> Unit)? = null,
    onSelect: (() -> Unit)? = null,
    moves: List<MoveCommand> = emptyList(),
    onMove: (MoveCommand) -> Unit = {},
    leading: (@Composable () -> Unit)? = null,
    onClick: () -> Unit,
) {
    var actions by remember { mutableStateOf(false) }
    val deleteLabel = stringResource(R.string.delete)
    val selectLabel = stringResource(R.string.select)
    val moveLabels = moves.map { it to it.label() }
    val line = HomeViewModel.status(document, canFindNames)
    val status = when (line) {
        StatusLine.NotRecognized -> stringResource(R.string.status_not_recognized)
        StatusLine.ImportNotFinished -> stringResource(R.string.status_import_not_finished)
        StatusLine.ReadyToFind -> stringResource(R.string.status_ready_to_find)
        StatusLine.ReadyToMark -> stringResource(R.string.status_ready_to_mark)
        is StatusLine.ToReview -> stringResource(R.string.status_to_review, line.count)
        StatusLine.ReadyToShare -> stringResource(R.string.status_ready_to_share)
        StatusLine.NothingFound -> stringResource(R.string.status_nothing_found)
    }
    // The status as a symbol and words, never a colour of its own:
    // the Project's dot is the row's one colour. Something to review reads strongest.
    val toReview = line is StatusLine.ToReview
    val tone = if (toReview) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant
    // The Project's name only when it differs from the title.
    val projectName = project?.name?.takeIf { it != document.title }
    val hasActions = onDelete != null || onSelect != null || moves.isNotEmpty()
    // Delete alone keeps its own name; with the moves, the press opens a menu.
    val longPressLabel = if (moves.isEmpty()) deleteLabel else stringResource(R.string.more_options)
    Box {
        ListItem(
            modifier = Modifier
                .combinedClickable(
                    onClick = onClick,
                    onLongClick = if (hasActions) ({ actions = true }) else null,
                    onLongClickLabel = if (hasActions) longPressLabel else null,
                )
                .semantics {
                    customActions = listOfNotNull(onSelect?.let { CustomAccessibilityAction(selectLabel) { it(); true } }) +
                        moveLabels.map { (command, label) -> CustomAccessibilityAction(label) { onMove(command); true } } +
                        listOfNotNull(onDelete?.let { CustomAccessibilityAction(deleteLabel) { it(); true } })
                },
            leadingContent = leading ?: project?.let {
                {
                    ProjectDot(it)
                }
            },
            headlineContent = { Text(document.title, maxLines = 2, overflow = TextOverflow.Ellipsis) },
            supportingContent = {
                Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    projectName?.let { Text(stringResource(R.string.project_named, it), maxLines = 1, overflow = TextOverflow.Ellipsis) }
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        Icon(statusIcon(line), contentDescription = null, tint = tone, modifier = Modifier.size(16.dp))
                        Text(status, color = tone, fontWeight = if (toReview) FontWeight.SemiBold else null)
                    }
                }
            },
        )
        DropdownMenu(expanded = actions, onDismissRequest = { actions = false }) {
            onSelect?.let { select ->
                DropdownMenuItem(
                    text = { Text(selectLabel) },
                    onClick = {
                        actions = false
                        select()
                    },
                )
            }
            MoveMenuItems(moves) { command ->
                actions = false
                onMove(command)
            }
            onDelete?.let { delete ->
                DropdownMenuItem(
                    text = { Text(deleteLabel) },
                    onClick = {
                        actions = false
                        delete()
                    },
                )
            }
        }
    }
}

/** A Project's colour: it marks the Project, never a state. */
@Composable
fun ProjectDot(project: ProjectSummary, size: Int = 10) {
    Box(Modifier.size(size.dp).background(Color(Palette.project(project.colour)), CircleShape))
}

/** iOS's status symbols, in Material's core set. */
private fun statusIcon(line: StatusLine): ImageVector = when (line) {
    StatusLine.ImportNotFinished -> Icons.Outlined.Refresh
    StatusLine.NotRecognized -> Icons.Outlined.Search
    StatusLine.ReadyToFind -> Icons.Outlined.Person
    StatusLine.ReadyToMark -> Icons.Outlined.Edit
    is StatusLine.ToReview -> Icons.Outlined.Info
    StatusLine.ReadyToShare, StatusLine.NothingFound -> Icons.Outlined.CheckCircle
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

/** What mdoc does, under "No documents yet" while there are none. */
@Composable
private fun EmptyHome(canFindNames: Boolean, empty: Boolean, modifier: Modifier = Modifier) {
    EmptyState(
        if (empty) stringResource(R.string.home_empty_title) else null,
        stringResource(if (canFindNames) R.string.home_empty_body else R.string.home_empty_body_below_floor),
        R.drawable.ic_find_in_page,
        modifier,
    )
}

/** A page's empty state: a symbol, a title and a line, centred. */
@Composable
fun EmptyState(title: String?, body: String?, icon: Int, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier.fillMaxWidth().padding(32.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Icon(
            painterResource(icon),
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.size(48.dp),
        )
        title?.let { Text(it, style = MaterialTheme.typography.titleLarge, textAlign = TextAlign.Center, modifier = Modifier.semantics { heading() }) }
        body?.let {
            Text(it, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center)
        }
    }
}

/**
 * Adding a document: pasted text, a title and a Project in one sheet. It
 * lands in Unfiled, or in the Project whose page it was started from,
 * unless another is chosen; no import makes a Project (#1091 Decision 35).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AddSheet(
    adding: PendingAdd,
    projects: List<ProjectSummary>,
    onAdd: (text: String?, title: String, project: ProjectId?) -> Unit,
    onDismiss: () -> Unit,
    initialProject: ProjectId? = null,
) {
    val pasting = adding is PendingAdd.Text
    var text by rememberSaveable { mutableStateOf("") }
    var title by rememberSaveable { mutableStateOf(adding.suggestedTitle) }
    // Until the title is typed, it follows the pasted text's first line (as on iOS).
    var titleTyped by rememberSaveable { mutableStateOf(false) }
    val fallback = stringResource(R.string.pasted_text)
    fun setText(value: String) {
        text = value
        if (pasting && !titleTyped) title = HomeViewModel.suggestedTitle(value, fallback)
    }
    var project by rememberSaveable { mutableStateOf(initialProject?.raw?.takeIf { id -> projects.any { it.id.raw == id } }) }
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(24.dp, 0.dp, 24.dp, 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(
                stringResource(if (pasting) R.string.paste_text else R.string.add_document),
                style = MaterialTheme.typography.titleLarge,
                modifier = Modifier.semantics { heading() },
            )
            if (pasting) {
                PasteFromClipboard(onPaste = ::setText)
                OutlinedTextField(
                    value = text,
                    onValueChange = ::setText,
                    label = { Text(stringResource(R.string.paste_placeholder)) },
                    modifier = Modifier.fillMaxWidth().height(200.dp),
                )
            }
            OutlinedTextField(
                value = title,
                onValueChange = {
                    title = it
                    titleTyped = true
                },
                label = { Text(stringResource(R.string.title)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Text(stringResource(R.string.project_label), style = MaterialTheme.typography.titleSmall)
            ProjectOption(stringResource(R.string.unfiled), null, project == null) { project = null }
            projects.forEach { option -> ProjectOption(option.name, option, project == option.id.raw) { project = option.id.raw } }
            Text(
                stringResource(R.string.project_explanation),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Button(
                onClick = {
                    val name = title.trim().ifEmpty { adding.suggestedTitle }
                    onAdd(if (pasting) text else null, name, project?.let(::ProjectId))
                },
                enabled = !pasting || text.isNotBlank(),
                modifier = Modifier.fillMaxWidth(),
            ) { Text(stringResource(R.string.add)) }
        }
    }
}

@Composable
private fun ProjectOption(label: String, project: ProjectSummary?, selected: Boolean, onClick: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth().clickable(onClick = onClick)) {
        RadioButton(selected = selected, onClick = onClick)
        project?.let { ProjectDot(it) }
        Text(label, modifier = Modifier.padding(start = 8.dp), style = MaterialTheme.typography.bodyLarge)
    }
}

/**
 * iOS's paste button: the clipboard's text into the field in one tap.
 * Offered only while the clipboard holds text; its description is read
 * without reading the text, so Android shows no paste notice until the tap.
 */
@Composable
fun PasteFromClipboard(onPaste: (String) -> Unit) {
    val context = LocalContext.current
    val clipboard = remember { context.getSystemService(ClipboardManager::class.java) }
    fun holdsText() = clipboard.primaryClipDescription?.let {
        it.hasMimeType(ClipDescription.MIMETYPE_TEXT_PLAIN) || it.hasMimeType(ClipDescription.MIMETYPE_TEXT_HTML)
    } == true
    var hasText by remember { mutableStateOf(holdsText()) }
    // Text copied while the sheet is open turns the button on.
    DisposableEffect(clipboard) {
        val listener = ClipboardManager.OnPrimaryClipChangedListener { hasText = holdsText() }
        clipboard.addPrimaryClipChangedListener(listener)
        onDispose { clipboard.removePrimaryClipChangedListener(listener) }
    }
    FilledTonalButton(
        onClick = { clipboard.primaryClip?.getItemAt(0)?.coerceToText(context)?.toString()?.let(onPaste) },
        enabled = hasText,
    ) {
        Icon(painterResource(R.drawable.ic_content_paste), contentDescription = null, modifier = Modifier.size(18.dp))
        Spacer(Modifier.width(8.dp))
        Text(stringResource(R.string.paste))
    }
}
