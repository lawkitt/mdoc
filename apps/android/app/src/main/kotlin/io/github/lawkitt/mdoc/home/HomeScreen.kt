package io.github.lawkitt.mdoc.home

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
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

/**
 * Home: the stored documents, or how to add the first one. A library that
 * could not be opened says so, rather than looking empty; `loaded` is false
 * until the first read.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(
    rows: List<HomeRow>,
    canFindNames: Boolean,
    modifier: Modifier = Modifier,
    loaded: Boolean = true,
    error: String? = null,
    onRetry: () -> Unit = {},
) {
    Scaffold(
        modifier = modifier,
        topBar = { TopAppBar(title = { Text(stringResource(R.string.app_name)) }) },
    ) { padding ->
        if (error != null) {
            CoreFailed(error, onRetry, Modifier.padding(padding))
        } else if (!loaded) {
            Box(Modifier.padding(padding).fillMaxSize())
        } else if (rows.isEmpty()) {
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
private fun CoreFailed(error: String, onRetry: () -> Unit, modifier: Modifier = Modifier) {
    Column(
        modifier = modifier.fillMaxSize().padding(32.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            stringResource(R.string.core_failed),
            style = MaterialTheme.typography.titleLarge,
            modifier = Modifier.semantics { heading() },
        )
        Text(
            error,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
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
