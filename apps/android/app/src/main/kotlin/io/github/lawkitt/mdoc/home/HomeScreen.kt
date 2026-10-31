package io.github.lawkitt.mdoc.home

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.lawkitt.mdoc.R

/** One stored document as Home lists it, newest first (ADR 0087). */
data class HomeRow(val path: String, val title: String)

/** Home: the stored documents, or how to add the first one. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(rows: List<HomeRow>, canFindNames: Boolean, modifier: Modifier = Modifier) {
    Scaffold(
        modifier = modifier,
        topBar = { TopAppBar(title = { Text(stringResource(R.string.app_name)) }) },
    ) { padding ->
        if (rows.isEmpty()) {
            EmptyHome(canFindNames, Modifier.padding(padding))
        } else {
            LazyColumn(Modifier.padding(padding)) {
                items(rows, key = { it.path }) { row ->
                    ListItem(headlineContent = { Text(row.title) })
                }
            }
        }
    }
}

@Composable
private fun EmptyHome(canFindNames: Boolean, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier.fillMaxSize().padding(32.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            stringResource(R.string.home_empty_title),
            style = MaterialTheme.typography.titleLarge,
            modifier = Modifier.semantics { heading() },
        )
        Text(
            stringResource(
                if (canFindNames) R.string.home_empty_body else R.string.home_empty_body_below_floor,
            ),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}
