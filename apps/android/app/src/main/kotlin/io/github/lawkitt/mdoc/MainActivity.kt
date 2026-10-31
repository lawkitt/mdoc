package io.github.lawkitt.mdoc

import android.Manifest
import android.content.pm.PackageManager
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
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
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
import io.github.lawkitt.mdoc.storage.MdocStorage
import io.github.lawkitt.mdoc.ui.MdocTheme
import java.io.File
import java.util.UUID
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    private val home: HomeViewModel by viewModels { HomeViewModel.factory(application as MdocApplication) }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        setContent { MdocTheme { App(application as MdocApplication, home) } }
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
    var open by rememberSaveable { mutableStateOf<String?>(null) }
    var readAfterOpen by rememberSaveable { mutableStateOf(false) }

    // Notification permission is asked at the first long job; the job runs either way (ADR 0089).
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
    // Scan: the system camera, a page per photo (ADR 0089); mdoc itself asks for no camera permission.
    val pages = remember { mutableStateListOf<File>() }
    var askNextPage by remember { mutableStateOf(false) }
    var pendingPage by remember { mutableStateOf<File?>(null) }
    val camera = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { taken ->
        val page = pendingPage
        if (taken && page != null && page.length() > 0) pages += page else page?.delete()
        askNextPage = pages.isNotEmpty()
    }
    fun takePage() {
        val folder = pages.firstOrNull()?.parentFile
            ?: File(app.core.storage.dir(MdocStorage.Area.Originals), UUID.randomUUID().toString()).apply { mkdirs() }
        val page = File(folder, "page-${pages.size + 1}.jpg")
        pendingPage = page
        camera.launch(FileProvider.getUriForFile(context, "${context.packageName}.files", page))
    }

    val current = open
    if (current == null) {
        HomeScreen(
            state = state,
            onOpen = { open = it.raw },
            onAdd = { option ->
                when (option) {
                    AddOption.Paste -> home.startPaste(context.getString(R.string.pasted_text))
                    AddOption.Files -> files.launch(OPENABLE)
                    AddOption.Photos -> photos.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
                    AddOption.Scan -> {
                        pages.clear()
                        takePage()
                    }
                }
            },
            onRetry = home::refresh,
        )
    } else {
        BackHandler { open = null }
        val documents = app.core.documents
        val session = remember(current) { runCatching { documents.session(DocumentId(current)) }.getOrNull() }
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
                    model.recognize(context.getString(R.string.recognizing_title, session.snapshot().title))
                }
            }
            DocumentScreen(
                model = model,
                beforeJob = ::beforeJob,
                onBack = {
                    open = null
                    home.refresh()
                },
            )
        }
    }

    state.adding?.let { adding ->
        AddSheet(
            adding = adding,
            cases = state.cases,
            onAdd = { text, title, case ->
                scope.launch {
                    home.add(text, title, case)?.let {
                        // Read at once, from the visible screen, as a long job.
                        readAfterOpen = true
                        open = it.raw
                    }
                }
            },
            onDismiss = home::cancelAdd,
        )
    }
    if (askNextPage) {
        AlertDialog(
            onDismissRequest = {},
            title = { Text(stringResource(R.string.scan)) },
            text = { Text(stringResource(R.string.pages_scanned, pages.size)) },
            confirmButton = {
                TextButton(onClick = {
                    askNextPage = false
                    home.scanned(pages.toList(), context.getString(R.string.scan))
                }) { Text(stringResource(R.string.done)) }
            },
            dismissButton = {
                TextButton(onClick = {
                    askNextPage = false
                    takePage()
                }) { Text(stringResource(R.string.add_page)) }
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
