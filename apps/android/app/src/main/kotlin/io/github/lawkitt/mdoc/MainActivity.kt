package io.github.lawkitt.mdoc

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.core.content.ContextCompat
import androidx.core.content.IntentCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.document.DocumentScreen
import io.github.lawkitt.mdoc.document.DocumentViewModel
import io.github.lawkitt.mdoc.home.AddOption
import io.github.lawkitt.mdoc.home.AddSheet
import io.github.lawkitt.mdoc.home.HomeScreen
import io.github.lawkitt.mdoc.home.HomeViewModel
import io.github.lawkitt.mdoc.settings.SettingsScreen
import io.github.lawkitt.mdoc.setup.CellularQuestion
import io.github.lawkitt.mdoc.setup.SetupCard
import io.github.lawkitt.mdoc.ui.MdocTheme
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private val home: HomeViewModel by viewModels { HomeViewModel.factory(application as MdocApplication) }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        if (savedInstanceState == null) receive(intent)
        setContent { MdocTheme { App(application as MdocApplication, home) } }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        receive(intent)
    }

    /** "Open in mdoc": a PDF, DOCX or image shared or opened from another app. */
    private fun receive(intent: Intent?) {
        val uris = when (intent?.action) {
            Intent.ACTION_VIEW -> listOfNotNull(intent.data)
            // IntentCompat: the typed getters are Android 13+; mdoc runs from 12.
            Intent.ACTION_SEND -> listOfNotNull(IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java))
            Intent.ACTION_SEND_MULTIPLE -> IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
            else -> emptyList()
        }
        home.received(contentResolver, uris, null, getString(R.string.title))
    }

    override fun onStart() {
        super.onStart()
        home.refresh()
    }
}

/** MIME types mdoc opens from Files: PDF, DOCX and images. */
private val OPENABLE = arrayOf(
    "application/pdf",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "image/*",
)

@Composable
private fun App(app: MdocApplication, home: HomeViewModel) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val state by home.state.collectAsStateWithLifecycle()
    val setupState by app.setup.state.collectAsStateWithLifecycle()
    // Setup reads the core's models once the library has opened.
    LaunchedEffect(state.loaded, state.error) {
        if (state.loaded && state.error == null) {
            app.setup.refresh()
        }
    }
    var open by rememberSaveable { mutableStateOf<String?>(null) }
    var settings by rememberSaveable { mutableStateOf(false) }
    var readAfterOpen by rememberSaveable { mutableStateOf(false) }

    // Notification permission is asked at the first long job; the job runs either way.
    val notifications = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {}
    fun beforeJob() {
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            notifications.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }

    val files = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let { home.received(context.contentResolver, listOf(it), null, context.getString(R.string.title)) }
    }
    val photos = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia()) { uris ->
        home.received(context.contentResolver, uris, io.github.lawkitt.mdoc.core.SourceKind.Image, context.getString(R.string.photo))
    }
    // The system camera receives a seekable in-memory output URI.
    val camera = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { taken ->
        home.cameraResult(taken)
    }
    fun takePage() {
        val uri = home.takePage(context) ?: return
        try {
            camera.launch(uri)
        } catch (error: RuntimeException) {
            home.cameraResult(false, error.message)
        }
    }

    val current = open
    if (settings) {
        BackHandler { settings = false }
        SettingsScreen(
            setup = setupState,
            version = BuildConfig.VERSION_NAME,
            onDownload = {
                beforeJob()
                app.setup.download { home.refresh() }
            },
            onCancel = app.setup::cancel,
            onUseCellular = app.setup::askCellular,
            onBack = { settings = false },
        )
    } else if (current == null) {
        HomeScreen(
            onSettings = { settings = true },
            state = state,
            onOpen = { open = it.raw },
            onDelete = home::delete,
            onAdd = { option ->
                when (option) {
                    AddOption.Paste -> home.startPaste(context.getString(R.string.pasted_text))
                    AddOption.Files -> files.launch(OPENABLE)
                    AddOption.Photos -> photos.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
                    AddOption.Scan -> takePage()
                }
            },
            onRetry = home::refresh,
            setupCard = if (setupState.showsCard || (setupState.working && state.loaded)) {
                {
                    SetupCard(
                        state = setupState,
                        onDownload = {
                            beforeJob()
                            app.setup.download { home.refresh() }
                        },
                        onNotNow = app.setup::dismissCard,
                        onCancel = app.setup::cancel,
                        onUseCellular = app.setup::askCellular,
                    )
                }
            } else {
                null
            },
        )
    } else {
        val documents = app.core.documents
        val session = remember(current) { runCatching { documents.session(DocumentId(current)) }.getOrNull() }
        // Closing the document wipes its decrypted original (#1091 Decision 24).
        fun close() {
            session?.closeOriginal()
            open = null
        }
        BackHandler(onBack = ::close)
        if (session == null) {
            LaunchedEffect(current) { open = null }
        } else {
            val model: DocumentViewModel = viewModel(
                key = current,
                factory = viewModelFactory {
                    initializer { DocumentViewModel(documents, session, app.jobs, app.core.models.snapshot().detector != null) }
                },
            )
            LaunchedEffect(current, readAfterOpen) {
                if (readAfterOpen) {
                    readAfterOpen = false
                    beforeJob()
                    model.recognize()
                }
            }
            DocumentScreen(
                model = model,
                beforeJob = ::beforeJob,
                setup = app.setup,
                onBack = {
                    close()
                    home.refresh()
                },
            )
        }
    }

    state.adding?.let { adding ->
        AddSheet(
            adding = adding,
            projects = state.projects,
            onAdd = { text, title, project ->
                scope.launch {
                    home.add(text, title, project)?.let {
                        // Read at once, from the visible screen, as a long job.
                        readAfterOpen = true
                        open = it.raw
                    }
                }
            },
            onDismiss = home::cancelAdd,
        )
    }
    CellularQuestion(setupState, app.setup::answerCellular)
    val scanTitle = stringResource(R.string.scan)
    if (state.scanPageCount > 0) {
        AlertDialog(
            onDismissRequest = {},
            title = { Text(scanTitle) },
            text = { Text(pluralStringResource(R.plurals.pages_scanned, state.scanPageCount, state.scanPageCount)) },
            confirmButton = {
                TextButton(onClick = { home.finishScan(scanTitle) }) { Text(stringResource(R.string.done)) }
            },
            dismissButton = {
                TextButton(onClick = ::takePage) { Text(stringResource(R.string.add_page)) }
            },
        )
    }
    state.refused?.let { reason ->
        AlertDialog(
            onDismissRequest = home::refusalShown,
            confirmButton = { TextButton(onClick = home::refusalShown) { Text(stringResource(R.string.ok)) } },
            title = { Text(stringResource(R.string.could_not_open, reason)) },
            text = { Text(stringResource(R.string.opens_files)) },
        )
    }
}
