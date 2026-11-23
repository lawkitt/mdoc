package io.github.lawkitt.mdoc.home

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationDrawerItem
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.lawkitt.mdoc.R
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.DocumentSummary

/**
 * The sidebar (#1091 Decision 33; docs/specs/mobile.md, Navigation): search
 * at the top, then Home, *To review* while names wait, the Projects (the
 * first five, All Projects and New Project), Unfiled and All Documents,
 * and Settings. Titles and Project names only. A modal drawer on a phone,
 * a standing one on a wide screen.
 */
@Composable
fun Sidebar(
    state: HomeState,
    destination: Destination,
    openDocument: DocumentId?,
    query: String,
    onQuery: (String) -> Unit,
    onDestination: (Destination) -> Unit,
    onDocument: (DocumentId) -> Unit,
    onNewProject: () -> Unit,
    onSettings: () -> Unit,
) {
    Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 8.dp)) {
        OutlinedTextField(
            value = query,
            onValueChange = onQuery,
            placeholder = { Text(stringResource(R.string.search_documents_and_projects), maxLines = 1, overflow = TextOverflow.Ellipsis) },
            leadingIcon = { Icon(Icons.Outlined.Search, contentDescription = null) },
            trailingIcon = if (query.isNotEmpty()) {
                { IconButton({ onQuery("") }) { Icon(Icons.Default.Clear, contentDescription = stringResource(R.string.cancel)) } }
            } else {
                null
            },
            singleLine = true,
            modifier = Modifier.fillMaxWidth().padding(bottom = 8.dp),
        )
        if (query.isNotBlank()) {
            val (documents, projects) = state.search(query)
            projects.forEach { project ->
                Item(project.name, destination == Destination.Project(project.id), icon = { ProjectDot(project) }) {
                    onDestination(Destination.Project(project.id))
                }
            }
            documents.forEach { document -> DocumentItem(state, document, openDocument, onDocument) }
            if (documents.isEmpty() && projects.isEmpty()) {
                Text(
                    stringResource(R.string.no_results, query.trim()),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(16.dp),
                )
            }
            return@Column
        }
        Item(stringResource(R.string.home), destination == Destination.Home && openDocument == null, icon = {
            Icon(Icons.Outlined.Home, contentDescription = null)
        }) { onDestination(Destination.Home) }
        val toReview = state.toReview
        if (toReview.isNotEmpty()) {
            Header(stringResource(R.string.to_review))
            toReview.forEach { DocumentItem(state, it, openDocument, onDocument) }
        }
        Header(stringResource(R.string.projects))
        state.topProjects.forEach { project ->
            Item(project.name, destination == Destination.Project(project.id) && openDocument == null, icon = { ProjectDot(project) }) {
                onDestination(Destination.Project(project.id))
            }
        }
        Item(
            stringResource(R.string.all_projects),
            destination == Destination.AllProjects && openDocument == null,
            icon = { Icon(painterResource(R.drawable.ic_work), contentDescription = null) },
            badge = state.projects.size.takeIf { it > 0 },
        ) { onDestination(Destination.AllProjects) }
        Item(stringResource(R.string.new_project), false, icon = { Icon(Icons.Default.Add, contentDescription = null) }, onClick = onNewProject)
        Spacer(Modifier.height(12.dp))
        Item(
            stringResource(R.string.unfiled),
            destination == Destination.Unfiled && openDocument == null,
            icon = { Icon(painterResource(R.drawable.ic_inbox), contentDescription = null) },
            badge = state.unfiled.size.takeIf { it > 0 },
        ) { onDestination(Destination.Unfiled) }
        Item(
            stringResource(R.string.all_documents),
            destination == Destination.AllDocuments && openDocument == null,
            icon = { Icon(painterResource(R.drawable.ic_description), contentDescription = null) },
        ) { onDestination(Destination.AllDocuments) }
        HorizontalDivider(Modifier.padding(vertical = 8.dp))
        Item(stringResource(R.string.settings), false, icon = { Icon(Icons.Outlined.Settings, contentDescription = null) }, onClick = onSettings)
    }
}

@Composable
private fun Header(title: String) {
    Text(
        title,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(16.dp, 16.dp, 16.dp, 8.dp).semantics { heading() },
    )
}

@Composable
private fun DocumentItem(state: HomeState, document: DocumentSummary, openDocument: DocumentId?, onDocument: (DocumentId) -> Unit) {
    val project = state.project(document.projectId)
    Item(document.title, openDocument == document.id, icon = {
        Box(Modifier.size(24.dp), contentAlignment = Alignment.Center) { project?.let { ProjectDot(it) } }
    }) { onDocument(document.id) }
}

@Composable
private fun Item(label: String, selected: Boolean, icon: @Composable () -> Unit, badge: Int? = null, onClick: () -> Unit) {
    NavigationDrawerItem(
        label = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        selected = selected,
        onClick = onClick,
        icon = icon,
        badge = badge?.let { { Text("$it") } },
    )
}
