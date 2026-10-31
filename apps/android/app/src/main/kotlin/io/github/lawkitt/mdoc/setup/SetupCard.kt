package io.github.lawkitt.mdoc.setup

import android.text.format.Formatter
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.lawkitt.mdoc.R
import io.github.lawkitt.mdoc.core.DeviceTier
import uniffi.mdoc_ffi.ModelId

/**
 * The first-use setup card (ADR 0026, 0085): what downloads and how large,
 * on Wi-Fi unless the user allows cellular; each model's state while it runs.
 * `continues`: a document asked for it, so the button says "& continue".
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun SetupCard(
    state: SetupState,
    onDownload: () -> Unit,
    onNotNow: (() -> Unit)?,
    onCancel: () -> Unit,
    onUseCellular: () -> Unit,
    modifier: Modifier = Modifier,
    continues: Boolean = false,
) {
    val context = LocalContext.current
    val size = Formatter.formatShortFileSize(context, state.missingBytes)
    val detector = state.snapshot.tier == DeviceTier.Detector
    Card(modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(
                stringResource(if (detector) R.string.setup_names_title else R.string.setup_ocr_title),
                style = MaterialTheme.typography.titleMedium,
                modifier = Modifier.semantics { heading() },
            )
            Text(
                stringResource(if (detector) R.string.setup_names_body else R.string.setup_ocr_body, size),
                style = MaterialTheme.typography.bodyMedium,
            )
            state.snapshot.offers.forEach { offer ->
                val label = stringResource(if (offer.id == ModelId.GLINER2_FP16) R.string.name_detector else R.string.ocr_english_russian)
                val progress = state.progress[offer.id]
                val status = when {
                    offer.installed -> stringResource(R.string.installed)
                    progress == null -> stringResource(R.string.not_downloaded)
                    state.waitingForWiFi -> stringResource(R.string.waiting_for_wifi)
                    state.installing && progress >= 0.999f -> stringResource(R.string.checking_and_installing)
                    else -> stringResource(R.string.downloading)
                }
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text("$label · ${Formatter.formatShortFileSize(context, offer.bytes)} · $status", style = MaterialTheme.typography.bodySmall)
                    if (progress != null && !offer.installed) LinearProgressIndicator({ progress }, Modifier.fillMaxWidth())
                }
            }
            state.error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            // Buttons wrap onto new lines rather than squeeze, at any text size.
            if (state.working) {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (state.waitingForWiFi) OutlinedButton(onUseCellular) { Text(stringResource(R.string.use_cellular)) }
                    TextButton(onCancel) { Text(stringResource(R.string.cancel_download)) }
                }
            } else {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button(onDownload) {
                        Text(stringResource(if (continues) R.string.download_and_continue else R.string.download_size, size))
                    }
                    onNotNow?.let { TextButton(it) { Text(stringResource(R.string.not_now)) } }
                }
            }
        }
    }
}

/** "Download over cellular?" (ADR 0088): asked on any cellular connection. */
@Composable
fun CellularQuestion(state: SetupState, onAnswer: (useCellular: Boolean) -> Unit) {
    if (!state.asksCellular) return
    val size = Formatter.formatShortFileSize(LocalContext.current, state.missingBytes)
    AlertDialog(
        onDismissRequest = { onAnswer(false) },
        title = { Text(stringResource(R.string.download_over_cellular, size)) },
        text = { Text(stringResource(R.string.carrier_may_charge)) },
        confirmButton = { TextButton({ onAnswer(true) }) { Text(stringResource(R.string.use_cellular)) } },
        dismissButton = { TextButton({ onAnswer(false) }) { Text(stringResource(R.string.wait_for_wifi)) } },
    )
}
