package io.github.lawkitt.mdoc.settings

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.lawkitt.mdoc.R
import io.github.lawkitt.mdoc.core.DeviceTier
import io.github.lawkitt.mdoc.setup.SetupCard
import io.github.lawkitt.mdoc.setup.SetupState

private const val SOURCE = "https://github.com/lawkitt/mdoc"
private const val PERMISSION = "https://github.com/lawkitt/mdoc/blob/main/LICENSE-APP-STORES.md"

/**
 * Settings (ADR 0085, 0088): the models and their downloads, where the
 * documents live, and About with the licence and the app-store permission
 * (ADR 0086). Below the detector floor it says the phone cannot find names.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    setup: SetupState,
    version: String,
    onDownload: () -> Unit,
    onCancel: () -> Unit,
    onUseCellular: () -> Unit,
    onBack: () -> Unit,
) {
    val context = LocalContext.current
    fun open(url: String) = context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.settings)) },
                navigationIcon = { IconButton(onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back)) } },
            )
        },
    ) { padding ->
        Column(
            Modifier.padding(padding).fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Heading(R.string.models)
            if (setup.snapshot.tier == DeviceTier.BelowFloor) Body(stringResource(R.string.below_floor_note))
            SetupCard(setup, onDownload = onDownload, onNotNow = null, onCancel = onCancel, onUseCellular = onUseCellular)
            Body(stringResource(R.string.models_explanation))
            HorizontalDivider()
            Body(stringResource(R.string.stays_on_phone))
            HorizontalDivider()
            Heading(R.string.about_mdoc)
            Body("${stringResource(R.string.version)} $version")
            TextButton({ open(SOURCE) }) { Text(stringResource(R.string.source_code)) }
            Body(stringResource(R.string.no_data))
            HorizontalDivider()
            Heading(R.string.licence)
            Body(stringResource(R.string.gpl_notice))
            Body(stringResource(R.string.app_store_permission))
            TextButton({ open(PERMISSION) }) { Text(stringResource(R.string.permission_text)) }
        }
    }
}

@Composable
private fun Heading(text: Int) {
    Text(stringResource(text), style = MaterialTheme.typography.titleMedium, modifier = Modifier.semantics { heading() })
}

@Composable
private fun Body(text: String) {
    Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
}
