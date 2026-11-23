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
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.PermanentDrawerSheet
import androidx.compose.material3.PermanentNavigationDrawer
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.core.content.IntentCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.ProjectDeletion
import io.github.lawkitt.mdoc.core.ProjectId
import io.github.lawkitt.mdoc.document.DocumentScreen
import io.github.lawkitt.mdoc.document.DocumentViewModel
import io.github.lawkitt.mdoc.home.AddOption
import io.github.lawkitt.mdoc.home.AddSheet
import io.github.lawkitt.mdoc.home.AllProjectsScreen
import io.github.lawkitt.mdoc.home.DeleteProjectDialog
import io.github.lawkitt.mdoc.home.Destination
import io.github.lawkitt.mdoc.home.DocumentListScreen
import io.github.lawkitt.mdoc.home.HomeScreen
import io.github.lawkitt.mdoc.home.HomeViewModel
import io.github.lawkitt.mdoc.home.NewProjectDialog
import io.github.lawkitt.mdoc.home.ProjectPage
import io.github.lawkitt.mdoc.home.RenameProjectDialog
import io.github.lawkitt.mdoc.home.RepeatedImportDialog
import io.github.lawkitt.mdoc.home.Sidebar
import io.github.lawkitt.mdoc.settings.SettingsScreen
import io.github.lawkitt.mdoc.setup.CellularQuestion
import io.github.lawkitt.mdoc.setup.SetupCard
import io.github.lawkitt.mdoc.ui.MdocTheme
import kotlinx.coroutines.Dispatchers
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

    /** A Delete still offering Undo happens when mdoc leaves the screen, as on iOS. */
    override fun onStop() {
        super.onStop()
        home.commitPendingDelete()
    }
}

/** MIME types mdoc opens from Files: PDF, DOCX and images. */
private val OPENABLE = arrayOf(
    "application/pdf",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "image/*",
)

/** Material's expanded width, from which the sidebar stands beside the page. */
private const val STANDING_SIDEBAR_DP = 840

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
    // What the main area shows (one owner): a top-level destination, and a
    // document open over it.
    var destination by rememberSaveable(stateSaver = Destination.Saver) { mutableStateOf<Destination>(Destination.Home) }
    var open by rememberSaveable { mutableStateOf<String?>(null) }
    var settings by rememberSaveable { mutableStateOf(false) }
    var readAfterOpen by rememberSaveable { mutableStateOf(false) }
    var query by rememberSaveable { mutableStateOf("") }
    /** New Project, Rename… or Delete Project… asking: "new", "rename:<id>", "delete:<id>". */
    var dialog by rememberSaveable { mutableStateOf<String?>(null) }
    val snackbar = remember { SnackbarHostState() }
    val drawer = rememberDrawerState(DrawerValue.Closed)
    val standing = LocalConfiguration.current.screenWidthDp >= STANDING_SIDEBAR_DP

    fun closeDrawer() {
        query = ""
        if (!standing) scope.launch { drawer.close() }
    }
    fun go(to: Destination) {
        destination = to
        open = null
        settings = false
        closeDrawer()
    }
    fun openDocument(id: DocumentId) {
        open = id.raw
        settings = false
        closeDrawer()
    }
    val onMenu: (() -> Unit)? = if (standing) null else ({ scope.launch { drawer.open() } })

    // A Project that went (deleted here or elsewhere) gives way to All Projects.
    val shownProject = destination.project
    LaunchedEffect(shownProject, state.projects, state.loaded) {
        if (shownProject != null && state.loaded && state.error == null && state.project(shownProject) == null) {
            destination = Destination.AllProjects
        }
    }

    // Delete's snackbar: Undo, else the documents go when it ends.
    val pendingDelete = state.pendingDelete
    val deletedOne = pendingDelete?.title?.let { stringResource(R.string.deleted_named, it) }
    val deletedMany = pendingDelete?.ids?.size?.let { pluralStringResource(R.plurals.deleted_documents, it, it) }
    val undo = stringResource(R.string.undo)
    LaunchedEffect(pendingDelete?.serial) {
        if (pendingDelete == null) return@LaunchedEffect
        val result = snackbar.showSnackbar(deletedOne ?: deletedMany.orEmpty(), actionLabel = undo, duration = SnackbarDuration.Long)
        if (result == SnackbarResult.ActionPerformed) home.undoDelete() else home.commitPendingDelete()
    }
    val deletedProject = state.deletedProject?.let { stringResource(R.string.deleted_named, it) }
    LaunchedEffect(deletedProject) {
        if (deletedProject == null) return@LaunchedEffect
        snackbar.showSnackbar(deletedProject)
        home.deletedProjectShown()
    }

    // Notification permission is asked at the first long job; the job runs either way.
    val notifications = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {}
    fun beforeJob() {
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            notifications.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }

    // What a picker returns lands where the picker was opened from.
    val files = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let { home.received(context.contentResolver, listOf(it), null, context.getString(R.string.title), home.pickingInto) }
    }
    val photos = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia()) { uris ->
        home.received(context.contentResolver, uris, io.github.lawkitt.mdoc.core.SourceKind.Image, context.getString(R.string.photo), home.pickingInto)
    }
    // The system camera receives a seekable in-memory output URI.
    val camera = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { taken ->
        home.cameraResult(taken)
    }
    fun takePage() {
        val uri = home.takePage(context, if (open == null) destination.project else null) ?: return
        try {
            camera.launch(uri)
        } catch (error: RuntimeException) {
            home.cameraResult(false, error.message)
        }
    }
    // Started from a Project's page, it lands in that Project (#1091 Decision 35).
    val onAdd: (AddOption) -> Unit = { option ->
        val into = destination.project
        when (option) {
            AddOption.Paste -> home.startPaste(context.getString(R.string.pasted_text), into)
            AddOption.Files -> {
                home.pickingInto = into
                files.launch(OPENABLE)
            }
            AddOption.Photos -> {
                home.pickingInto = into
                photos.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
            }
            AddOption.Scan -> takePage()
        }
    }
    val setupCard: (@Composable () -> Unit)? = if (setupState.showsCard || (setupState.working && state.loaded)) {
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
    }

    val current = open
    val main: @Composable () -> Unit = {
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
            // Back from a sidebar page goes Home, Material's start
            // destination; from Home it leaves mdoc.
            destination.back?.let { back -> BackHandler { destination = back } }
            Page(
                destination = destination,
                state = state,
                home = home,
                snackbar = snackbar,
                onMenu = onMenu,
                onOpen = ::openDocument,
                onAdd = onAdd,
                onGo = ::go,
                onDialog = { dialog = it },
                onSettings = { settings = true },
                setupCard = setupCard,
            )
        } else {
            val documents = app.core.documents
            val session = remember(current) { runCatching { documents.session(DocumentId(current)) }.getOrNull() }
            // Leaving the document, however it goes (Back, or another document
            // opened in its place), wipes its decrypted original (#1091 Decision 24).
            DisposableEffect(session) { onDispose { session?.closeOriginal() } }
            BackHandler {
                open = null
                home.refresh()
            }
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
                        open = null
                        home.refresh()
                    },
                )
            }
        }
    }
    val sidebar: @Composable () -> Unit = {
        Sidebar(
            state = state,
            destination = destination,
            openDocument = current?.let(::DocumentId),
            query = query,
            onQuery = { query = it },
            onDestination = ::go,
            onDocument = ::openDocument,
            onNewProject = {
                closeDrawer()
                dialog = "new"
            },
            onSettings = {
                closeDrawer()
                settings = true
            },
        )
    }
    if (standing) {
        PermanentNavigationDrawer(drawerContent = { PermanentDrawerSheet(Modifier.width(300.dp)) { sidebar() } }) {
            Box(Modifier.fillMaxSize()) { main() }
        }
    } else {
        ModalNavigationDrawer(
            drawerState = drawer,
            // A document's own horizontal gestures keep the edge.
            gesturesEnabled = current == null || drawer.isOpen,
            drawerContent = { ModalDrawerSheet(Modifier.width(320.dp)) { sidebar() } },
        ) { main() }
        // The drawer closes on Back before anything else does: registered
        // after the page's own Back, so it is asked first.
        if (drawer.isOpen) BackHandler { closeDrawer() }
    }

    state.adding?.let { adding ->
        AddSheet(
            adding = adding,
            projects = state.projects,
            // Started from a Project's page, it starts on that Project (#1091 Decision 35).
            initialProject = adding.into,
            onAdd = { text, title, project ->
                // On the main thread: opening the document changes the screens.
                scope.launch(Dispatchers.Main) {
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
    state.repeated?.let { repeated ->
        RepeatedImportDialog(
            repeated,
            onOpenExisting = { home.openExisting()?.let(::openDocument) },
            onImportAgain = home::importAgain,
            onCancel = home::cancelRepeated,
        )
    }
    ProjectDialogs(dialog, state, home, onDone = { dialog = null }, onCreated = { go(Destination.Project(it)) }, onDeleting = { id, choice ->
        // A document deleted with its Project closes first.
        if (choice == ProjectDeletion.DeleteDocuments && state.documentsIn(id).any { it.id.raw == open }) open = null
    })
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

/** The page a top-level destination shows. */
@Composable
private fun Page(
    destination: Destination,
    state: io.github.lawkitt.mdoc.home.HomeState,
    home: HomeViewModel,
    snackbar: SnackbarHostState,
    onMenu: (() -> Unit)?,
    onOpen: (DocumentId) -> Unit,
    onAdd: (AddOption) -> Unit,
    onGo: (Destination) -> Unit,
    onDialog: (String) -> Unit,
    onSettings: () -> Unit,
    setupCard: (@Composable () -> Unit)?,
) {
    when (destination) {
        Destination.Home -> HomeScreen(
            state = state,
            onOpen = onOpen,
            onAdd = onAdd,
            onRetry = home::refresh,
            setupCard = setupCard,
            onSettings = onSettings,
            onMenu = onMenu,
            snackbar = snackbar,
        )
        Destination.AllProjects -> AllProjectsScreen(
            state = state,
            onOpen = { onGo(Destination.Project(it)) },
            onNewProject = { onDialog("new") },
            onRename = { onDialog("rename:${it.raw}") },
            onDelete = { onDialog("delete:${it.raw}") },
            onMenu = onMenu,
            snackbar = snackbar,
        )
        Destination.Unfiled -> DocumentListScreen(
            title = stringResource(R.string.unfiled),
            documents = state.unfiled,
            state = state,
            onOpen = onOpen,
            onAdd = onAdd,
            onDelete = home::delete,
            showsProject = false,
            selectable = true,
            emptyTitle = R.string.nothing_in_unfiled,
            emptyBody = R.string.unfiled_body,
            onMenu = onMenu,
            snackbar = snackbar,
        )
        Destination.AllDocuments -> DocumentListScreen(
            title = stringResource(R.string.all_documents),
            documents = state.listed,
            state = state,
            onOpen = onOpen,
            onAdd = onAdd,
            onDelete = home::delete,
            searchable = true,
            onMenu = onMenu,
            snackbar = snackbar,
        )
        is Destination.Project -> {
            val project = state.project(destination.id)
            if (project == null) {
                Box(Modifier.fillMaxSize())
            } else {
                ProjectPage(
                    state = state,
                    project = project,
                    onOpen = onOpen,
                    onAdd = onAdd,
                    onDelete = home::delete,
                    onRename = { onDialog("rename:${project.id.raw}") },
                    onDeleteProject = { onDialog("delete:${project.id.raw}") },
                    onLoad = { home.loadPage(project.id) },
                    onRemoveListed = { wording, always -> home.removeListed(project.id, wording, always) },
                    onShowReply = home::showSavedReply,
                    onHideReply = home::hideSavedReply,
                    onMenu = onMenu,
                    snackbar = snackbar,
                )
            }
        }
    }
}

/** New Project, Rename… and Delete Project… (#1091 Decisions 9 and 34). */
@Composable
private fun ProjectDialogs(
    dialog: String?,
    state: io.github.lawkitt.mdoc.home.HomeState,
    home: HomeViewModel,
    onDone: () -> Unit,
    onCreated: (ProjectId) -> Unit,
    onDeleting: (ProjectId, ProjectDeletion) -> Unit,
) {
    val scope = rememberCoroutineScope()
    when {
        dialog == "new" -> NewProjectDialog(
            onCreate = { name ->
                onDone()
                // On the main thread: what follows changes the screens.
                scope.launch(Dispatchers.Main) { home.createProject(name)?.let(onCreated) }
            },
            onDismiss = onDone,
        )
        dialog?.startsWith("rename:") == true -> {
            val id = ProjectId(dialog.removePrefix("rename:"))
            // Gone meanwhile: nothing to ask.
            val project = state.project(id) ?: return
            RenameProjectDialog(project.name, onRename = { name ->
                onDone()
                home.renameProject(id, name)
            }, onDismiss = onDone)
        }
        dialog?.startsWith("delete:") == true -> {
            val id = ProjectId(dialog.removePrefix("delete:"))
            val project = state.project(id) ?: return
            DeleteProjectDialog(project, state.documentsIn(id).size, onDelete = { choice ->
                onDone()
                onDeleting(id, choice)
                scope.launch(Dispatchers.Main) { home.deleteProject(id, choice) }
            }, onDismiss = onDone)
        }
    }
}
