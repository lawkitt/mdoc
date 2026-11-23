package io.github.lawkitt.mdoc.home

import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.lawkitt.mdoc.R
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.ProjectDeletion
import io.github.lawkitt.mdoc.core.ProjectId
import io.github.lawkitt.mdoc.core.ProjectNameRow
import io.github.lawkitt.mdoc.core.ProjectSummary
import io.github.lawkitt.mdoc.core.RestoreResult
import io.github.lawkitt.mdoc.core.SavedReplyRow
import io.github.lawkitt.mdoc.document.copy
import io.github.lawkitt.mdoc.document.shareIntent
import io.github.lawkitt.mdoc.ui.Palette
import io.github.lawkitt.mdoc.ui.RestoredText
import java.text.DateFormat
import java.util.Date

/** "3 documents · 1 to review", as iOS's All Projects counts a Project. */
@Composable
private fun projectCount(state: HomeState, project: ProjectId): String {
    val documents = state.documentsIn(project)
    val toReview = documents.count { it.phase == io.github.lawkitt.mdoc.core.DocumentPhase.Reviewing }
    val base = pluralStringResource(R.plurals.document_count, documents.size, documents.size)
    return if (toReview > 0) base + " · " + stringResource(R.string.status_to_review, toReview) else base
}

/**
 * All Projects (the sidebar's All Projects row): each Project, pinned
 * first, then most recently updated, with its colour and how many
 * documents it holds; a tap opens its page, a long press offers Rename and
 * Delete.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AllProjectsScreen(
    state: HomeState,
    onOpen: (ProjectId) -> Unit,
    onNewProject: () -> Unit,
    onRename: (ProjectId) -> Unit,
    onDelete: (ProjectId) -> Unit,
    onMenu: (() -> Unit)? = null,
    snackbar: SnackbarHostState = remember { SnackbarHostState() },
) {
    Scaffold(
        snackbarHost = { SnackbarHost(snackbar) },
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.all_projects)) },
                navigationIcon = { onMenu?.let { MenuButton(it) } },
                actions = {
                    IconButton(onNewProject) { Icon(Icons.Default.Add, contentDescription = stringResource(R.string.new_project)) }
                },
            )
        },
    ) { padding ->
        val inner = Modifier.padding(padding).fillMaxWidth().wrapContentWidth().widthIn(max = 720.dp)
        if (state.loaded && state.projects.isEmpty()) {
            Column(inner.verticalScroll(rememberScrollState())) {
                EmptyState(stringResource(R.string.no_projects_yet), stringResource(R.string.no_projects_body), R.drawable.ic_work)
            }
            return@Scaffold
        }
        LazyColumn(inner) {
            items(state.projects, key = { it.id.raw }) { project ->
                ProjectRow(project, projectCount(state, project.id), { onOpen(project.id) }, { onRename(project.id) }, { onDelete(project.id) })
            }
        }
    }
}

@Composable
private fun ProjectRow(project: ProjectSummary, count: String, onOpen: () -> Unit, onRename: () -> Unit, onDelete: () -> Unit) {
    var actions by remember { mutableStateOf(false) }
    val rename = stringResource(R.string.rename_ellipsis)
    val delete = stringResource(R.string.delete_project_ellipsis)
    Box {
        ListItem(
            modifier = Modifier
                .combinedClickable(onClick = onOpen, onLongClick = { actions = true })
                .semantics {
                    customActions = listOf(
                        CustomAccessibilityAction(rename) { onRename(); true },
                        CustomAccessibilityAction(delete) { onDelete(); true },
                    )
                },
            leadingContent = { ProjectDot(project, 12) },
            headlineContent = { Text(project.name, maxLines = 2, overflow = TextOverflow.Ellipsis) },
            supportingContent = { Text(count) },
        )
        DropdownMenu(expanded = actions, onDismissRequest = { actions = false }) {
            ProjectMenuItems(onRename = { actions = false; onRename() }, onDelete = { actions = false; onDelete() })
        }
    }
}

/** ⋯: Rename… and Delete Project… (Colour, Categories and the lists' own commands are #1125). */
@Composable
private fun ProjectMenuItems(onRename: () -> Unit, onDelete: () -> Unit) {
    DropdownMenuItem(
        text = { Text(stringResource(R.string.rename_ellipsis)) },
        leadingIcon = { Icon(Icons.Outlined.Edit, contentDescription = null) },
        onClick = onRename,
    )
    DropdownMenuItem(
        text = { Text(stringResource(R.string.delete_project_ellipsis), color = MaterialTheme.colorScheme.error) },
        leadingIcon = { Icon(Icons.Outlined.Delete, contentDescription = null, tint = MaterialTheme.colorScheme.error) },
        onClick = onDelete,
    )
}

/** A Project page's three views (#1091 Decision 29). */
enum class ProjectSegment(val title: Int) { Documents(R.string.documents), Replies(R.string.replies), Names(R.string.names) }

/**
 * A Project's page (#1091 Decision 34): its colour and name in the bar,
 * never "Documents"; Documents (first), Replies and Names; an Add button
 * whose documents land in it; and a ⋯ menu with Rename and Delete.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ProjectPage(
    state: HomeState,
    project: ProjectSummary,
    onOpen: (DocumentId) -> Unit,
    onAdd: (AddOption) -> Unit,
    onDelete: (DocumentId) -> Unit,
    onRename: () -> Unit,
    onDeleteProject: () -> Unit,
    onLoad: () -> Unit,
    onRemoveListed: (String, Boolean) -> Unit,
    onShowReply: (SavedReplyRow) -> Unit,
    onHideReply: () -> Unit,
    onMenu: (() -> Unit)? = null,
    snackbar: SnackbarHostState = remember { SnackbarHostState() },
) {
    var segment by rememberSaveable(project.id.raw) { mutableStateOf(ProjectSegment.Documents) }
    var query by rememberSaveable(project.id.raw) { mutableStateOf("") }
    var menu by remember { mutableStateOf(false) }
    // Replies and names are read again whenever the library changes: a reply
    // Restore with mdoc saved, a name a review added.
    LaunchedEffect(project.id, state.documents) { onLoad() }
    val page = state.page?.takeIf { it.project == project.id }
    Scaffold(
        snackbarHost = { SnackbarHost(snackbar) },
        topBar = {
            TopAppBar(
                // The Project's colour and name, never "Documents".
                title = {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.semantics(mergeDescendants = true) { heading() },
                    ) {
                        ProjectDot(project, 12)
                        Text(project.name, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(start = 10.dp))
                    }
                },
                navigationIcon = { onMenu?.let { MenuButton(it) } },
                actions = {
                    Box {
                        IconButton({ menu = true }) { Icon(Icons.Default.MoreVert, contentDescription = stringResource(R.string.more_options)) }
                        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                            ProjectMenuItems(onRename = { menu = false; onRename() }, onDelete = { menu = false; onDeleteProject() })
                        }
                    }
                },
            )
        },
        floatingActionButton = { AddButton(onAdd) },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize()) {
            PrimaryTabRow(selectedTabIndex = segment.ordinal) {
                ProjectSegment.entries.forEach { option ->
                    Tab(selected = segment == option, onClick = { segment = option }, text = { Text(stringResource(option.title)) })
                }
            }
            val inner = Modifier.fillMaxWidth().wrapContentWidth().widthIn(max = 720.dp)
            when (segment) {
                ProjectSegment.Documents -> {
                    val documents = state.documentsIn(project.id)
                    if (documents.isEmpty()) {
                        Column(inner.verticalScroll(rememberScrollState())) {
                            EmptyState(stringResource(R.string.home_empty_title), stringResource(R.string.project_no_documents_body), R.drawable.ic_description)
                        }
                    } else {
                        LazyColumn(inner) {
                            items(documents, key = { it.id.raw }) { document ->
                                DocumentRow(document, null, state.canFindNames, onDelete = { onDelete(document.id) }) { onOpen(document.id) }
                            }
                        }
                    }
                }
                ProjectSegment.Replies -> Replies(page?.replies.orEmpty(), inner, onShowReply)
                ProjectSegment.Names -> Names(page, query, { query = it }, inner, onRemoveListed)
            }
        }
    }
    state.shownReply?.let { shown -> SavedReplySheet(shown, onHideReply) }
}

/** Replies (#1091 Decision 31): the Project's saved replies, newest first; a tap shows one restored. */
@Composable
private fun Replies(replies: List<SavedReplyRow>, modifier: Modifier, onShow: (SavedReplyRow) -> Unit) {
    if (replies.isEmpty()) {
        Column(modifier.verticalScroll(rememberScrollState())) {
            EmptyState(stringResource(R.string.no_replies_yet), stringResource(R.string.no_replies_body), R.drawable.ic_inbox)
        }
        return
    }
    val format = remember { DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT) }
    LazyColumn(modifier) {
        items(replies, key = { it.id }) { reply ->
            ListItem(
                modifier = Modifier.clickable { onShow(reply) },
                headlineContent = { Text(reply.title, maxLines = 2, overflow = TextOverflow.Ellipsis) },
                supportingContent = { Text(format.format(Date(reply.created))) },
            )
        }
    }
}

/**
 * Names (#1091 Decision 29): searchable, grouped by category, then Always
 * replace and Never replace. A wording is removed with its visible Remove;
 * nothing is added here, as on desktop.
 */
@Composable
private fun Names(
    page: ProjectPageData?,
    query: String,
    onQuery: (String) -> Unit,
    modifier: Modifier,
    onRemoveListed: (String, Boolean) -> Unit,
) {
    val details = page?.details ?: return
    val groups = HomeViewModel.names(details.names, query)
    val always = details.always.filter { HomeViewModel.wordings(listOf(it.wording), query).isNotEmpty() }
    val never = HomeViewModel.wordings(details.never, query)
    LazyColumn(modifier) {
        item(key = "search") {
            OutlinedTextField(
                value = query,
                onValueChange = onQuery,
                placeholder = { Text(stringResource(R.string.names_and_wordings)) },
                leadingIcon = { Icon(Icons.Outlined.Search, contentDescription = null) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().padding(16.dp, 8.dp),
            )
        }
        if (details.names.isEmpty() && query.isBlank()) {
            item(key = "empty") {
                EmptyState(stringResource(R.string.no_names_yet), stringResource(R.string.project_no_names_body), R.drawable.ic_find_in_page)
            }
        } else if (groups.isEmpty() && always.isEmpty() && never.isEmpty()) {
            item(key = "none-found") {
                EmptyState(stringResource(R.string.no_results, query.trim()), null, R.drawable.ic_find_in_page)
            }
        }
        for ((category, rows) in groups) {
            item(key = "category-$category") { SectionHeader(stringResource(category.label)) }
            items(rows, key = { "name-${it.alias}" }) { NameRow(it) }
        }
        if (query.isBlank() || always.isNotEmpty()) {
            item(key = "always") { SectionHeader(stringResource(R.string.always_replace)) }
            if (always.isEmpty()) item(key = "always-none") { NoneRow() }
            items(always, key = { "always-${it.wording}" }) { entry ->
                ListedRow(entry.wording, stringResource(entry.category.label)) { onRemoveListed(entry.wording, true) }
            }
        }
        if (query.isBlank() || never.isNotEmpty()) {
            item(key = "never") { SectionHeader(stringResource(R.string.never_replace)) }
            if (never.isEmpty()) item(key = "never-none") { NoneRow() }
            items(never, key = { "never-$it" }) { wording -> ListedRow(wording, null) { onRemoveListed(wording, false) } }
        }
    }
}

@Composable
private fun SectionHeader(title: String) {
    Text(
        title,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.fillMaxWidth().padding(16.dp, 16.dp, 16.dp, 4.dp).semantics { heading() },
    )
}

@Composable
private fun NoneRow() {
    Text(
        stringResource(R.string.none),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(16.dp, 4.dp),
    )
}

/** One of a Project's names: its alias as the text shows it, then the wordings it stands for. */
@Composable
private fun NameRow(name: ProjectNameRow) {
    ListItem(
        modifier = Modifier.semantics(mergeDescendants = true) {},
        headlineContent = { AliasTag(name.alias, name.category) },
        supportingContent = if (name.originals.isEmpty()) null else ({ Text(name.originals.joinToString(" · ")) }),
    )
}

/** An alias in its category's colour, as the text shows it. */
@Composable
private fun AliasTag(alias: String, category: Category) {
    val colour = Color(Palette.category(category, isSystemInDarkTheme()))
    Surface(shape = RoundedCornerShape(6.dp), color = colour.copy(alpha = 0.16f)) {
        Text(alias, style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(8.dp, 2.dp))
    }
}

/** An Always or Never wording, with Remove at its end. */
@Composable
private fun ListedRow(wording: String, detail: String?, onRemove: () -> Unit) {
    ListItem(
        headlineContent = { Text(wording) },
        supportingContent = detail?.let { { Text(it) } },
        trailingContent = {
            IconButton(onRemove) {
                Icon(Icons.Outlined.Delete, contentDescription = stringResource(R.string.remove_named, wording))
            }
        },
    )
}

/** A saved reply restored, from a Project's Replies: Copy and Share, as the Reply pane's bar has. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SavedReplySheet(shown: ShownReply, onDismiss: () -> Unit) {
    val context = LocalContext.current
    val reply = shown.reply
    var copied by remember(reply.id) { mutableStateOf(false) }
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(24.dp, 0.dp, 24.dp, 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(reply.title, style = MaterialTheme.typography.titleLarge, modifier = Modifier.semantics { heading() })
            val result = shown.restored
            when {
                result != null -> {
                    Card {
                        Text(
                            listOfNotNull(
                                pluralStringResource(R.plurals.values_restored, result.restoredCount, result.restoredCount),
                                if (result.unresolvedCount > 0) stringResource(R.string.unrecognised, result.unresolvedCount) else null,
                            ).joinToString(" · "),
                            modifier = Modifier.padding(12.dp),
                            style = MaterialTheme.typography.labelLarge,
                        )
                    }
                    RestoredText(result, null, Modifier.fillMaxWidth())
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        OutlinedButton(onClick = { context.startActivity(shareIntent(result.text, null, false, context)) }) {
                            Text(stringResource(R.string.share))
                        }
                        Button(onClick = {
                            copy(context, result.text, sensitive = true)
                            copied = true
                        }) { Text(stringResource(if (copied) R.string.copied else R.string.copy)) }
                    }
                }
                shown.failed -> Text(stringResource(R.string.nothing_to_restore), color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}

/** New Project: its name; Create is off while it is blank (#677). */
@Composable
fun NewProjectDialog(onCreate: (String) -> Unit, onDismiss: () -> Unit) {
    NameDialog(R.string.new_project, R.string.new_project_body, "", R.string.create, onCreate, onDismiss)
}

/** Rename… on a Project: its new name. */
@Composable
fun RenameProjectDialog(name: String, onRename: (String) -> Unit, onDismiss: () -> Unit) {
    NameDialog(R.string.rename_project, null, name, R.string.rename, onRename, onDismiss)
}

@Composable
private fun NameDialog(title: Int, body: Int?, initial: String, confirm: Int, onConfirm: (String) -> Unit, onDismiss: () -> Unit) {
    var name by rememberSaveable { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                body?.let { Text(stringResource(it)) }
                OutlinedTextField(value = name, onValueChange = { name = it }, label = { Text(stringResource(R.string.name)) }, singleLine = true)
            }
        },
        confirmButton = { TextButton(onClick = { onConfirm(name) }, enabled = name.isNotBlank()) { Text(stringResource(confirm)) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

/**
 * Delete Project… (#1091 Decision 9). With documents it asks what becomes
 * of them, saying the Project's own AI replies go either way (the
 * maintainer's answer on #1091); empty, it asks plainly, as desktop does.
 */
@Composable
fun DeleteProjectDialog(project: ProjectSummary, documents: Int, onDelete: (ProjectDeletion) -> Unit, onDismiss: () -> Unit) {
    if (documents == 0) {
        AlertDialog(
            onDismissRequest = onDismiss,
            title = { Text(stringResource(R.string.delete_project_question)) },
            text = { Text(stringResource(R.string.project_has_no_documents, project.name)) },
            confirmButton = {
                TextButton(onClick = { onDelete(ProjectDeletion.DeleteDocuments) }) {
                    Text(stringResource(R.string.delete), color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
        )
        return
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.delete_named_question, project.name)) },
        text = { Text(pluralStringResource(R.plurals.delete_project_body, documents, documents)) },
        // Three answers, stacked: Material's dialog for long button labels.
        confirmButton = {
            Column(horizontalAlignment = Alignment.End) {
                TextButton(onClick = { onDelete(ProjectDeletion.MoveToUnfiled) }) { Text(stringResource(R.string.move_documents_to_unfiled)) }
                TextButton(onClick = { onDelete(ProjectDeletion.DeleteDocuments) }) {
                    Text(stringResource(R.string.delete_them), color = MaterialTheme.colorScheme.error)
                }
                TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
            }
        },
    )
}

/** "Already in mdoc" (#1091 Decision 7), in desktop's words (#1107). */
@Composable
fun RepeatedImportDialog(repeated: RepeatedImport, onOpenExisting: () -> Unit, onImportAgain: () -> Unit, onCancel: () -> Unit) {
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text(stringResource(R.string.already_in_mdoc, repeated.name)) },
        text = { Text(stringResource(R.string.already_in_mdoc_body)) },
        confirmButton = {
            Column(horizontalAlignment = Alignment.End) {
                TextButton(onClick = onOpenExisting) { Text(stringResource(R.string.open_existing)) }
                TextButton(onClick = onImportAgain) { Text(stringResource(R.string.import_again)) }
                TextButton(onClick = onCancel) { Text(stringResource(R.string.cancel)) }
            }
        },
    )
}
