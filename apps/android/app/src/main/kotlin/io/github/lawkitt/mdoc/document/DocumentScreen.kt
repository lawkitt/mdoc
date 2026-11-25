package io.github.lawkitt.mdoc.document

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.PersistableBundle
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.lawkitt.mdoc.R
import io.github.lawkitt.mdoc.core.Candidate
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.Decision
import io.github.lawkitt.mdoc.core.DocumentId
import io.github.lawkitt.mdoc.core.DocumentPhase
import io.github.lawkitt.mdoc.core.HandOffRefusal
import io.github.lawkitt.mdoc.core.Identity
import io.github.lawkitt.mdoc.core.PromotedFrom
import io.github.lawkitt.mdoc.core.MentionState
import io.github.lawkitt.mdoc.core.OriginalSource
import io.github.lawkitt.mdoc.core.ReplySnapshot
import io.github.lawkitt.mdoc.core.RestoredSegment
import io.github.lawkitt.mdoc.core.Scope
import io.github.lawkitt.mdoc.core.ProjectSummary
import io.github.lawkitt.mdoc.home.HomeViewModel
import io.github.lawkitt.mdoc.home.MoveCommand
import io.github.lawkitt.mdoc.home.MoveMenuItems
import io.github.lawkitt.mdoc.home.PasteFromClipboard
import io.github.lawkitt.mdoc.home.SavedReplySheet
import io.github.lawkitt.mdoc.jobs.JobKind
import io.github.lawkitt.mdoc.setup.SetupCard
import io.github.lawkitt.mdoc.setup.SetupController
import io.github.lawkitt.mdoc.ui.OriginalView
import io.github.lawkitt.mdoc.ui.Palette
import io.github.lawkitt.mdoc.ui.RestoredText
import io.github.lawkitt.mdoc.ui.ReviewText
import io.github.lawkitt.mdoc.ui.ScanTime
import kotlinx.coroutines.launch
import uniffi.mdoc_ffi.JobStage

/**
 * One document: its text with candidates, the primary action that follows
 * its state, and the hand-off bar once nothing is left to decide. The twin of the iOS `DocumentScreen`.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DocumentScreen(
    model: DocumentViewModel,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    beforeJob: () -> Unit = {},
    setup: SetupController? = null,
    /** The app's snackbar, which a move's Undo shows on. */
    snackbar: SnackbarHostState = remember { SnackbarHostState() },
    /** Every Project, for the moves its menu offers. */
    projects: List<ProjectSummary> = emptyList(),
    onMove: ((MoveCommand) -> Unit)? = null,
    /** Moves and Undos finished: the document reads itself again at each. */
    moves: Long = 0,
    /** Opens another document in this one's place: one Promote to Document made. */
    onOpenDocument: (DocumentId) -> Unit = {},
    /** Settings › AI connector (preview): Ask AI and "Paste reply?" (#1164). */
    aiConnector: Boolean = false,
) {
    val state by model.state.collectAsStateWithLifecycle()
    val job by model.running.collectAsStateWithLifecycle()
    val interrupted by model.interrupted.collectAsStateWithLifecycle()
    val snapshot = state.snapshot
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var menu by remember { mutableStateOf(false) }
    // A move rewrote its text and changed its Project.
    LaunchedEffect(moves) { if (moves > 0) model.refresh() }
    val moveCommands = if (onMove == null) emptyList() else HomeViewModel.moveCommands(snapshot.projectId, model.saved, projects)
    var showsOriginal by rememberSaveable { mutableStateOf(false) }
    val reviewScroll = rememberScrollState()

    LaunchedEffect(state.notice) {
        state.notice?.let {
            snackbar.showSnackbar(context.getString(it))
            model.noticeShown()
        }
    }
    // The job's result arrives as a new snapshot when it ends.
    LaunchedEffect(job) { if (job == null) model.refresh() }
    LifecycleResumeEffect(model) {
        model.resumed()
        // Back from the AI app Ask AI shared to, "Paste reply?" shows.
        model.askAiReturned()
        onPauseOrDispose {}
    }
    // Ask AI's chooser says when it started an app (the twin of iOS's
    // completed share sheet); only while the preview is on.
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(model, aiConnector) {
        if (!aiConnector) return@DisposableEffect onDispose {}
        val app = context.applicationContext
        val receiver = registerAskAiChosen(app) { model.askAiChosen(lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) }
        onDispose { app.unregisterReceiver(receiver) }
    }
    // The preview turned off: nothing of Ask AI waits on.
    LaunchedEffect(aiConnector) { if (!aiConnector && state.replyOffer != null) model.dismissReplyBanner() }
    // The banner shows over the text: on a phone the Original gives way.
    LaunchedEffect(state.showsReplyBanner) { if (state.showsReplyBanner) showsOriginal = false }

    // The hand-off's ways out, each through Copy for AI's check (#1091
    // Decision 19); a refusal shows its own words and step (#1209).
    val refusal = model.handOffRefusal(state)
    val step = refusal?.let(model::step)
    val resources = LocalResources.current
    val share: (asFile: Boolean) -> Unit = { asFile ->
        scope.launch { model.handOff()?.let { context.startActivity(shareIntent(it.text, it.fileName, asFile, context)) } }
    }
    val copyForAi: () -> Unit = {
        scope.launch {
            model.handOff()?.let {
                copy(context, it.text)
                model.notice(R.string.copied_for_ai)
            }
        }
    }
    val take: (HandOffRefusal.Step) -> Unit = { taken ->
        when (taken) {
            is HandOffRefusal.Step.ReplaceAndCopy -> scope.launch {
                model.replaceAndHandOff()?.let {
                    copy(context, it.text)
                    model.notice(R.string.copied_for_ai)
                }
            }
            HandOffRefusal.Step.Review -> model.review()
            HandOffRefusal.Step.FindNames -> {
                model.dismissRefusal()
                beforeJob()
                model.findNames()
            }
        }
    }

    Scaffold(
        modifier = modifier,
        snackbarHost = { SnackbarHost(snackbar) },
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text(snapshot.title, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        // A promoted document says where it came from; the way back is in ⋯ (#1091 Decision 3).
                        snapshot.promotedFrom?.let { from ->
                            Text(
                                stringResource(from.label),
                                style = MaterialTheme.typography.labelMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.testTag("from-reply-line"),
                            )
                        }
                    }
                },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
                    }
                },
                actions = {
                    if (snapshot.identities.isNotEmpty()) {
                        IconButton(onClick = { model.showIdentities(true) }) {
                            Icon(Icons.Default.Person, contentDescription = stringResource(R.string.identities))
                        }
                    }
                    IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, contentDescription = stringResource(R.string.more_options)) }
                    DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                        // The hand-off's second way out (a long press on iOS):
                        // always listed; until the document can leave,
                        // disabled with why, and the step that resolves it
                        // after them, as on iOS (#1209).
                        if (!state.showsReply) {
                            val goes = refusal == null && model.primaryAction(state, job) == PrimaryAction.HandOff
                            val why = refusal?.reason(resources)
                            HandOffMenuItem(R.string.copy_for_ai, why, goes, "menu-copy-for-ai") {
                                menu = false
                                copyForAi()
                            }
                            HandOffMenuItem(R.string.share, why, goes, "menu-share") {
                                menu = false
                                share(false)
                            }
                            HandOffMenuItem(R.string.share_as_file, why, goes, "menu-share-file") {
                                menu = false
                                share(true)
                            }
                            step?.let {
                                DropdownMenuItem(
                                    text = { Text(it.title(resources)) },
                                    // Find names waits while a job runs, as the bar does.
                                    enabled = it != HandOffRefusal.Step.FindNames || job == null,
                                    modifier = Modifier.testTag("menu-hand-off-step"),
                                    onClick = {
                                        menu = false
                                        take(it)
                                    },
                                )
                            }
                        }
                        // The document's own notes, and the reply it was promoted
                        // from (#1091 Decision 3). The core keeps notes with the
                        // document it has read: until then nothing typed could be kept.
                        val read = snapshot.phase != DocumentPhase.Recognizing
                        DropdownMenuItem(
                            text = {
                                Column {
                                    Text(stringResource(R.string.notes))
                                    if (!read) {
                                        Text(
                                            stringResource(R.string.once_the_document_is_read),
                                            style = MaterialTheme.typography.bodySmall,
                                        )
                                    }
                                }
                            },
                            enabled = read,
                            modifier = Modifier.testTag("document-notes"),
                            onClick = {
                                menu = false
                                model.showNotes()
                            },
                        )
                        snapshot.promotedFrom?.let { from ->
                            DropdownMenuItem(
                                text = { Text(stringResource(from.label)) },
                                // Once the reply is deleted there is nothing to go back to.
                                enabled = from is PromotedFrom.Reply,
                                modifier = Modifier.testTag("from-reply"),
                                onClick = {
                                    menu = false
                                    model.showFromReply()
                                },
                            )
                        }
                        HorizontalDivider()
                        // iOS's title menu: Move to Project, before Delete.
                        MoveMenuItems(moveCommands) { command ->
                            menu = false
                            onMove?.invoke(command)
                        }
                        DropdownMenuItem(
                            text = { Text(stringResource(R.string.delete)) },
                            onClick = {
                                menu = false
                                model.delete(onBack)
                            },
                        )
                    }
                },
            )
        },
        bottomBar = {
            Column {
                // Back from the AI app Ask AI shared to (#1164 decision 11).
                if (aiConnector && state.showsReplyBanner) {
                    ReplyBanner(
                        onPaste = { if (it.isNotBlank()) model.restoreReply(it) },
                        onType = { model.showReplySheet(true) },
                        onClose = model::dismissReplyBanner,
                    )
                }
                // The password card's Unlock is the way on while it asks, as on iOS.
                if (state.passwordPrompt == null || job != null) ActionBar(
                    action = model.primaryAction(state, job),
                    refusal = refusal,
                    step = step,
                    onStep = take,
                    askAi = if (aiConnector) {
                        { scope.launch { model.askAi()?.let { context.startActivity(askAiIntent(it, context)) } } }
                    } else {
                        null
                    },
                    hasReply = snapshot.reply != null,
                    showsReply = state.showsReply,
                    onRecognize = {
                        beforeJob()
                        model.recognize()
                    },
                    onFindNames = {
                        beforeJob()
                        model.findNames()
                    },
                    onCancel = model::cancelJob,
                    onReplaceAll = model::replaceAll,
                    onShare = share,
                    onCopy = copyForAi,
                    onPasteReply = { model.showReplySheet(true) },
                    onShareReply = { snapshot.reply?.let { context.startActivity(shareIntent(it.restored.text, null, false, context)) } },
                    onCopyReply = {
                        snapshot.reply?.let {
                            copy(context, it.restored.text, sensitive = true)
                            model.notice(R.string.copied_reply)
                        }
                    },
                    onShowReply = model::showReply,
                )
            }
        },
    ) { padding ->
        // The keyboard takes its height from the screen (edge to edge draws
        // under it otherwise), so the password card's field stays above it.
        BoxWithConstraints(Modifier.padding(padding).consumeWindowInsets(padding).imePadding().fillMaxSize()) {
        // The panes take the height the notices leave; the notices and the
        // setup card take at most half of it and scroll on their own, so their
        // buttons stay reachable on a short screen or at large text.
        val noticesHeight = maxHeight / 2
        Column(Modifier.fillMaxSize()) {
            Column(Modifier.heightIn(max = noticesHeight).verticalScroll(rememberScrollState())) {
            if (model.isMine(interrupted?.documentId)) {
                Notice(
                    stringResource(
                        when {
                            interrupted?.kind == JobKind.Scan -> R.string.finding_stopped_background
                            // A PDF or DOCX still importing, as on iOS.
                            !snapshot.kind.needsRecognition -> R.string.import_stopped_background
                            else -> R.string.recognition_stopped_background
                        },
                    ),
                )
            }
            if (!model.findsNames && snapshot.phase == DocumentPhase.Unscanned) {
                val reason = setup?.state?.collectAsStateWithLifecycle()?.value?.snapshot?.belowFloor
                Notice(ScanTime.belowFloor(reason))
            } else if (snapshot.phase == DocumentPhase.Unscanned && job == null) {
                // Before a scan expected to take more than about a minute.
                val minutes = remember(snapshot.markdown) { model.scanEstimate() }
                minutes?.let { Notice(stringResource(R.string.scan_estimate, it)) }
            }
            snapshot.warning?.let { Notice(it) }
            val needs = state.needsSetup
            val prompt = state.passwordPrompt
            if (prompt != null && job == null) {
                PasswordCard(
                    prompt,
                    onUnlock = {
                        beforeJob()
                        model.unlock(it)
                    },
                    modifier = Modifier.padding(16.dp),
                )
            } else if (needs != null && setup != null) {
                val setupState by setup.state.collectAsStateWithLifecycle()
                // Download & continue: the job that asked for setup runs again.
                SetupCard(
                    state = setupState,
                    continues = true,
                    onDownload = {
                        beforeJob()
                        setup.download {
                            if (needs == JobKind.Scan) {
                                model.findNames()
                            } else {
                                model.recognize()
                            }
                        }
                    },
                    onNotNow = model::skipSetup,
                    onCancel = setup::cancel,
                    onUseCellular = setup::askCellular,
                    modifier = Modifier.padding(16.dp),
                )
            } else if (needs != null) {
                Notice(stringResource(R.string.download_models_first))
            }
            }
            // Text, Original and Reply: one at a time on a phone; on a wide
            // screen the original sits beside the text.
            val wide = LocalConfiguration.current.screenWidthDp >= 840
            val hasOriginal = snapshot.original != OriginalSource.None
            val panes = buildList {
                add(Pane.Text)
                if (hasOriginal && !wide) add(Pane.Original)
                if (snapshot.reply != null) add(Pane.Reply)
            }
            val pane = when {
                state.showsReply && snapshot.reply != null -> Pane.Reply
                showsOriginal && Pane.Original in panes -> Pane.Original
                else -> Pane.Text
            }
            if (panes.size > 1) {
                SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().padding(16.dp, 8.dp)) {
                    panes.forEachIndexed { index, option ->
                        SegmentedButton(
                            selected = pane == option,
                            onClick = {
                                showsOriginal = option == Pane.Original
                                model.showReply(option == Pane.Reply)
                            },
                            shape = SegmentedButtonDefaults.itemShape(index, panes.size),
                        ) { Text(stringResource(option.label)) }
                    }
                }
            }
            val text: @Composable (Modifier) -> Unit = { paneModifier ->
                if (snapshot.phase == DocumentPhase.Recognizing) {
                    // iOS's three states: a file being read, an import that
                    // stopped, a scan or photo waiting for (or part through) OCR.
                    val reading = job != null && model.isMine(job?.documentId)
                    when {
                        !snapshot.kind.needsRecognition && reading ->
                            RecognizingPane(R.string.no_text_yet, R.string.reading_the_file_body, paneModifier)
                        // The password card is the way on, not Finish import.
                        !snapshot.kind.needsRecognition && state.passwordPrompt != null ->
                            RecognizingPane(R.string.status_import_not_finished, R.string.import_not_finished_locked_body, paneModifier)
                        !snapshot.kind.needsRecognition ->
                            RecognizingPane(R.string.status_import_not_finished, R.string.import_not_finished_body, paneModifier)
                        snapshot.resumesOcr ->
                            RecognizingPane(R.string.no_text_yet, R.string.recognition_resumes_body, paneModifier)
                        else -> RecognizingPane(R.string.no_text_yet, R.string.recognition_runs_body, paneModifier)
                    }
                } else {
                    ReviewText(
                        markdown = snapshot.markdown,
                        styling = snapshot.styling,
                        candidates = snapshot.candidates,
                        selected = state.reviewing,
                        onCandidate = model::open,
                        onReplaceSelection = model::startManualReplace,
                        // Room below the last line while a candidate's sheet is open, so
                        // a mention at the very end can still scroll above the sheet.
                        modifier = paneModifier.verticalScroll(reviewScroll).padding(16.dp)
                            .padding(bottom = if (state.reviewing != null) noticesHeight else 0.dp),
                        // Near the top, above the candidate's sheet.
                        onSelectedTop = { top ->
                            scope.launch { reviewScroll.animateScrollTo((top - reviewScroll.viewportSize / 6).coerceAtLeast(0)) }
                        },
                    )
                }
            }
            Box(Modifier.weight(1f).fillMaxWidth()) {
                when {
                    pane == Pane.Reply -> Readable {
                        val reply = snapshot.reply!!
                        val promote: (() -> Unit)? = reply.id?.let { id -> { model.promote(id, onOpenDocument) } }
                        ReplyPane(
                            reply,
                            onPromote = promote,
                            onRemove = { reply.id?.let(model::clearReply) },
                        )
                    }
                    pane == Pane.Original -> OriginalView(snapshot.original, Modifier.fillMaxSize())
                    wide && hasOriginal -> Row(Modifier.fillMaxSize()) {
                        text(Modifier.weight(1f).fillMaxHeight())
                        OriginalView(snapshot.original, Modifier.weight(1f).fillMaxHeight())
                    }
                    else -> Readable { text(Modifier.fillMaxSize()) }
                }
            }
        }
        }
    }

    state.reviewing?.let { id ->
        snapshot.candidate(id)?.let { candidate ->
            CandidateSheet(
                candidate = candidate,
                mentionCount = snapshot.mentions(candidate.identityId).size,
                hasNext = snapshot.pendingCount > 1,
                onDecide = { decision, scope, next -> model.decide(id, decision, scope, next) },
                onDismiss = model::closeCandidate,
            )
        }
    }
    state.manual?.let { (_, text) ->
        ManualReplaceSheet(text, onReplace = model::replaceSelection, onDismiss = model::cancelManualReplace)
    }
    if (state.showsIdentities) {
        IdentitiesSheet(
            snapshot.identities,
            // An identity opens its first mention, as on iOS.
            onOpen = { identity ->
                snapshot.mentions(identity.id).firstOrNull()?.let {
                    model.showIdentities(false)
                    model.open(it.id)
                }
            },
            // Replace all or Keep all of an identity, as iOS's swipe and VoiceOver actions do.
            onDecide = { identity, decision ->
                snapshot.mentions(identity.id).firstOrNull()?.let { model.decide(it.id, decision, Scope.AllOccurrences) }
            },
            onDismiss = { model.showIdentities(false) },
        )
    }
    if (state.showsReplySheet) {
        ReplySheet(snapshot.projectName, onRestore = model::restoreReply, onDismiss = { model.showReplySheet(false) })
    }
    if (state.showsNotes) {
        NotesSheet(model)
    }
    state.fromReply?.let { shown ->
        SavedReplySheet(shown, onDismiss = model::hideFromReply, onPromote = { model.promote(shown.reply.id, onOpenDocument) })
    }
    // Copy for AI, Share or Ask AI refused on a tap: why, and its step (#1209).
    state.refusal?.let { refused ->
        val resolves = model.step(refused)
        val ok: @Composable () -> Unit = { TextButton(onClick = model::dismissRefusal) { Text(stringResource(R.string.ok)) } }
        AlertDialog(
            onDismissRequest = model::dismissRefusal,
            confirmButton = {
                if (resolves == null) {
                    ok()
                } else {
                    TextButton(onClick = { take(resolves) }, Modifier.testTag("refusal-step")) { Text(resolves.title(resources)) }
                }
            },
            dismissButton = if (resolves == null) null else ok,
            title = { Text(refused.reason(resources)) },
            text = { Text(refused.detail(resources)) },
            modifier = Modifier.testTag("refusal-dialog"),
        )
    }
    state.error?.let { message ->
        AlertDialog(
            onDismissRequest = model::dismissError,
            confirmButton = { TextButton(onClick = model::dismissError) { Text(stringResource(R.string.ok)) } },
            title = { Text(stringResource(R.string.something_went_wrong)) },
            text = { Text(message) },
        )
    }
}

/** Copy for AI or a Share in the document's menu: always listed, disabled with why until it goes. */
@Composable
private fun HandOffMenuItem(label: Int, why: String?, goes: Boolean, tag: String, onClick: () -> Unit) {
    DropdownMenuItem(
        text = {
            Column {
                Text(stringResource(label))
                if (why != null) Text(why, style = MaterialTheme.typography.bodySmall)
            }
        },
        enabled = goes,
        modifier = Modifier.testTag(tag),
        onClick = onClick,
    )
}

@Composable
private fun Notice(text: String) {
    Surface(color = MaterialTheme.colorScheme.secondaryContainer, modifier = Modifier.fillMaxWidth()) {
        Text(text, modifier = Modifier.padding(16.dp, 12.dp), style = MaterialTheme.typography.bodyMedium)
    }
}

/** About 90 characters: text that has a tablet's whole width keeps lines it can be read in. */
private val ReadingWidth = 720.dp

/** The pane at most [ReadingWidth] wide, centred; a phone is narrower anyway. */
@Composable
private fun Readable(content: @Composable () -> Unit) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        Box(Modifier.widthIn(max = ReadingWidth).fillMaxSize()) { content() }
    }
}

@Composable
private fun RecognizingPane(title: Int, body: Int, modifier: Modifier = Modifier.fillMaxSize()) {
    Column(
        modifier.padding(32.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            stringResource(title),
            style = MaterialTheme.typography.titleLarge,
            textAlign = TextAlign.Center,
            modifier = Modifier.semantics { heading() },
        )
        Text(
            stringResource(body),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

/** The bar under the document: its one primary action, or the hand-off once nothing is left to decide. */
@Composable
private fun ActionBar(
    action: PrimaryAction,
    /** Why the hand-off waits, before a tap; null when it goes. */
    refusal: HandOffRefusal?,
    step: HandOffRefusal.Step?,
    onStep: (HandOffRefusal.Step) -> Unit,
    /** Ask AI, in Share's place: the AI connector preview is on. */
    askAi: (() -> Unit)?,
    hasReply: Boolean,
    showsReply: Boolean,
    onRecognize: () -> Unit,
    onFindNames: () -> Unit,
    onCancel: () -> Unit,
    onReplaceAll: () -> Unit,
    onShare: (asFile: Boolean) -> Unit,
    onCopy: () -> Unit,
    onPasteReply: () -> Unit,
    onShareReply: () -> Unit,
    onCopyReply: () -> Unit,
    onShowReply: (Boolean) -> Unit,
) {
    Surface(tonalElevation = 3.dp) {
        // Clear of the gesture bar: the bar's colour reaches the screen's edge, its buttons do not.
        Column(
            Modifier.fillMaxWidth().navigationBarsPadding().wrapContentWidth().widthIn(max = ReadingWidth).padding(16.dp, 12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            when (action) {
                is PrimaryAction.Recognize -> Primary(
                    stringResource(if (action.resume) R.string.resume_recognition else R.string.recognize_text), onRecognize,
                )
                PrimaryAction.FinishImport -> Primary(stringResource(R.string.finish_import), onRecognize)
                PrimaryAction.FindNames -> Primary(stringResource(R.string.find_names), onFindNames)
                is PrimaryAction.Running -> {
                    Text(stageLabel(action.job.kind, action.job.stage, action.job.secondsLeft), style = MaterialTheme.typography.bodyMedium)
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                    OutlinedButton(onClick = onCancel, Modifier.fillMaxWidth()) { Text(stringResource(R.string.cancel)) }
                }
                PrimaryAction.Waiting -> Text(stringResource(R.string.waiting_for_other), style = MaterialTheme.typography.bodyMedium)
                is PrimaryAction.ReplaceAll -> Primary(stringResource(R.string.replace_count, action.count), onReplaceAll)
                // On the Reply tab the bar acts on the reply; TalkBack
                // hears the full names, the buttons show the short ones.
                PrimaryAction.HandOff -> if (showsReply) {
                    HandOffRow(
                        Triple(onShareReply, R.string.share, R.string.share_reply),
                        Triple(onCopyReply, R.string.copy, R.string.copy_reply),
                        Triple(onPasteReply, R.string.paste, R.string.paste_another_reply),
                    )
                } else if (refusal != null) {
                    // Every name decided, but the Vault refuses the text as
                    // saved: a known name written out, changed since its
                    // review, excluded from AI. Why, and its step in Share's
                    // place; Paste still restores a reply (#1209).
                    val resources = LocalResources.current
                    Text(
                        refusal.note(resources),
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.testTag("hand-off-refusal"),
                    )
                    if (step != null) {
                        HandOffButtons(
                            Triple({ onStep(step) }, step.title(resources), step.title(resources)),
                            Triple(onPasteReply, resources.getString(R.string.paste), resources.getString(R.string.paste_reply)),
                        )
                    } else {
                        OutlinedButton(onClick = onPasteReply, Modifier.fillMaxWidth().semantics { contentDescription = resources.getString(R.string.paste_reply) }) {
                            Text(stringResource(R.string.paste))
                        }
                    }
                } else if (askAi != null) {
                    HandOffRow(
                        Triple(askAi, R.string.ask_ai, R.string.ask_ai),
                        Triple(onCopy, R.string.copy, R.string.copy),
                        Triple(onPasteReply, R.string.paste, R.string.paste_reply),
                    )
                } else {
                    HandOffRow(
                        Triple({ onShare(false) }, R.string.share, R.string.share),
                        Triple(onCopy, R.string.copy, R.string.copy),
                        Triple(onPasteReply, R.string.paste, R.string.paste_reply),
                    )
                }
            }
        }
    }
}

/**
 * Share, Copy, Paste: one row, or stacked when a label would not fit on one
 * line, as at the largest text sizes. Each is (action, label,
 * what TalkBack reads).
 */
@Composable
private fun HandOffRow(vararg buttons: Triple<() -> Unit, Int, Int>) {
    val resources = LocalResources.current
    HandOffButtons(*buttons.map { (onClick, label, spoken) -> Triple(onClick, resources.getString(label), resources.getString(spoken)) }.toTypedArray())
}

/** [HandOffRow] of labels already in words. */
@Composable
private fun HandOffButtons(vararg buttons: Triple<() -> Unit, String, String>) {
    val stacked = LocalDensity.current.fontScale > 1.3f
    val items: @Composable (Modifier) -> Unit = { modifier ->
        buttons.forEachIndexed { index, (onClick, label, spoken) ->
            val described = modifier.semantics { contentDescription = spoken }
            val text: @Composable () -> Unit = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) }
            when (index) {
                0 -> Button(onClick, described) { text() }
                1 -> FilledTonalButton(onClick, described) { text() }
                else -> OutlinedButton(onClick, described) { text() }
            }
        }
    }
    if (stacked) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { items(Modifier.fillMaxWidth()) }
    } else {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) { items(Modifier.weight(1f)) }
    }
}

@Composable
private fun Primary(label: String, onClick: () -> Unit) {
    Button(onClick = onClick, modifier = Modifier.fillMaxWidth()) { Text(label) }
}

@Composable
private fun stageLabel(kind: JobKind, stage: JobStage?, secondsLeft: Long?): String {
    if (stage == JobStage.SCANNING && secondsLeft != null) {
        return stringResource(R.string.finding_names_left, ScanTime.left(secondsLeft))
    }
    return stringResource(
        when (stage) {
            JobStage.READING -> R.string.reading_the_file
            JobStage.CONVERTING -> R.string.converting
            JobStage.RECOGNIZING -> R.string.recognizing_text
            JobStage.SCANNING -> R.string.finding_names
            JobStage.COOLING_DOWN -> R.string.paused_to_cool_down
            null -> if (kind == JobKind.Scan) R.string.finding_names else R.string.reading_the_file
        },
    )
}

/** Tapping a candidate: its alias, Keep or Replace, all occurrences or just this one. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun CandidateSheet(
    candidate: Candidate,
    mentionCount: Int,
    hasNext: Boolean,
    onDecide: (Decision, Scope, Boolean) -> Unit,
    onDismiss: () -> Unit,
) {
    var replace by rememberSaveable(candidate.id.raw) { mutableStateOf(candidate.state != MentionState.Kept) }
    var scope by rememberSaveable(candidate.id.raw) { mutableStateOf(Scope.AllOccurrences) }
    val decision = if (replace) Decision.Replace else Decision.Keep
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(24.dp, 0.dp, 24.dp, 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(
                stringResource(candidate.category.label),
                style = MaterialTheme.typography.titleLarge,
                modifier = Modifier.semantics { heading() },
            )
            Text(candidate.original, style = MaterialTheme.typography.headlineSmall)
            Text(
                "${stringResource(R.string.alias)}: ${candidate.alias}",
                style = MaterialTheme.typography.bodyLarge,
                // The category's colour for this theme: the light one is too dim on dark.
                color = androidx.compose.ui.graphics.Color(Palette.category(candidate.category, isSystemInDarkTheme())),
            )
            SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                SegmentedButton(replace, { replace = true }, SegmentedButtonDefaults.itemShape(0, 2)) {
                    Text(stringResource(R.string.replace))
                }
                SegmentedButton(!replace, { replace = false }, SegmentedButtonDefaults.itemShape(1, 2)) {
                    Text(stringResource(R.string.keep))
                }
            }
            Text(
                stringResource(if (replace) R.string.alias_explanation else R.string.keep_explanation),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (mentionCount > 1) {
                Text(stringResource(R.string.apply_to), style = MaterialTheme.typography.titleSmall)
                Choice(stringResource(R.string.all_occurrences, mentionCount), scope == Scope.AllOccurrences) { scope = Scope.AllOccurrences }
                Choice(stringResource(R.string.just_this_one), scope == Scope.JustThisOne) { scope = Scope.JustThisOne }
            }
            // Side by side, or stacked when the labels would not fit.
            val buttons: @Composable (Modifier) -> Unit = { modifier ->
                Button({ onDecide(decision, scope, false) }, modifier) { Text(stringResource(R.string.done), maxLines = 1) }
                if (hasNext) {
                    OutlinedButton({ onDecide(decision, scope, true) }, modifier) {
                        Text(stringResource(R.string.next_proposal), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
            if (LocalDensity.current.fontScale > 1.3f) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { buttons(Modifier.fillMaxWidth()) }
            } else {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) { buttons(Modifier.weight(1f)) }
            }
        }
    }
}

@Composable
private fun Choice(label: String, selected: Boolean, onClick: () -> Unit) {
    Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
        RadioButton(selected = selected, onClick = onClick)
        Text(label, style = MaterialTheme.typography.bodyLarge)
    }
}

/** Replace… on a selection: its category; every exact repeat is replaced too. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ManualReplaceSheet(text: String, onReplace: (Category) -> Unit, onDismiss: () -> Unit) {
    var category by rememberSaveable { mutableStateOf(Category.Person) }
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(24.dp, 0.dp, 24.dp, 24.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.replace), style = MaterialTheme.typography.titleLarge, modifier = Modifier.semantics { heading() })
            Text(stringResource(R.string.selected_text), style = MaterialTheme.typography.labelLarge)
            Text(text, style = MaterialTheme.typography.bodyLarge)
            HorizontalDivider()
            Text(stringResource(R.string.category), style = MaterialTheme.typography.titleSmall)
            Category.entries.forEach { option ->
                Choice(stringResource(option.label), category == option) { category = option }
            }
            Text(
                stringResource(R.string.repeats_replaced),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Button({ onReplace(category) }, Modifier.fillMaxWidth()) { Text(stringResource(R.string.replace)) }
        }
    }
}

/** Identities by category, with their aliases and how they were decided. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun IdentitiesSheet(
    identities: List<Identity>,
    onOpen: (Identity) -> Unit,
    onDecide: (Identity, Decision) -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Text(
            stringResource(R.string.identities),
            style = MaterialTheme.typography.titleLarge,
            modifier = Modifier.padding(24.dp, 0.dp).semantics { heading() },
        )
        LazyColumn {
            for ((category, group) in identities.groupBy { it.category }.toSortedMap()) {
                item(key = "header-$category") {
                    Text(
                        stringResource(category.label),
                        style = MaterialTheme.typography.titleSmall,
                        modifier = Modifier.padding(24.dp, 12.dp, 24.dp, 4.dp).semantics { heading() },
                    )
                }
                items(group, key = { it.id.raw }) { identity ->
                    val dark = isSystemInDarkTheme()
                    var actions by remember { mutableStateOf(false) }
                    val replaceAll = stringResource(R.string.replace_all)
                    val keepAll = stringResource(R.string.keep_all)
                    Box {
                        ListItem(
                            // A tap opens the first mention; a long press, or TalkBack's
                            // actions, replace or keep all of them.
                            modifier = Modifier
                                .combinedClickable(
                                    onClickLabel = stringResource(R.string.opens_first_mention),
                                    onLongClick = { actions = true },
                                ) { onOpen(identity) }
                                .semantics {
                                    customActions = listOf(
                                        CustomAccessibilityAction(replaceAll) { onDecide(identity, Decision.Replace); true },
                                        CustomAccessibilityAction(keepAll) { onDecide(identity, Decision.Keep); true },
                                    )
                                },
                            // On the sheet's own colour, not banded rows.
                            colors = ListItemDefaults.colors(containerColor = androidx.compose.ui.graphics.Color.Transparent),
                            leadingContent = {
                                Box(
                                    Modifier.size(10.dp).background(
                                        androidx.compose.ui.graphics.Color(Palette.category(identity.category, dark)),
                                        CircleShape,
                                    ),
                                )
                            },
                            headlineContent = { Text(identity.primaryWording) },
                            supportingContent = {
                                Text(
                                    "${identity.alias} · ${stringResource(
                                        when (identity.state) {
                                            MentionState.Proposed -> R.string.proposed
                                            MentionState.Replaced -> R.string.replaced
                                            MentionState.Kept -> R.string.kept
                                        },
                                    )} · ${pluralStringResource(R.plurals.mentions, identity.mentionCount, identity.mentionCount)}",
                                )
                            },
                        )
                        DropdownMenu(expanded = actions, onDismissRequest = { actions = false }) {
                            DropdownMenuItem(text = { Text(replaceAll) }, onClick = {
                                actions = false
                                onDecide(identity, Decision.Replace)
                            })
                            DropdownMenuItem(text = { Text(keepAll) }, onClick = {
                                actions = false
                                onDecide(identity, Decision.Keep)
                            })
                        }
                    }
                }
            }
        }
    }
}

/**
 * "Paste reply?" over the bar, back from the AI app Ask AI shared to
 * (#1164 decision 11): Paste restores the answer on the clipboard in one
 * tap, as the bar's Paste sheet does; Type a reply opens that sheet for one
 * that is not. × puts it away; the bar's Paste stays.
 */
@Composable
private fun ReplyBanner(onPaste: (String) -> Unit, onType: () -> Unit, onClose: () -> Unit) {
    Surface(color = MaterialTheme.colorScheme.secondaryContainer, modifier = Modifier.fillMaxWidth().testTag("reply-banner")) {
        Column(
            Modifier.wrapContentWidth().widthIn(max = ReadingWidth).padding(16.dp, 4.dp, 4.dp, 12.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Row(verticalAlignment = Alignment.Top) {
                Text(
                    buildAnnotatedString {
                        withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(stringResource(R.string.paste_reply_question)) }
                        append(" ")
                        append(stringResource(R.string.paste_reply_banner))
                    },
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.weight(1f).padding(top = 12.dp),
                )
                IconButton(onClick = onClose, modifier = Modifier.testTag("reply-banner-close")) {
                    Icon(Icons.Default.Close, contentDescription = stringResource(R.string.close))
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(end = 12.dp)) {
                Box(Modifier.testTag("reply-banner-paste")) { PasteFromClipboard(onPaste = onPaste) }
                OutlinedButton(onClick = onType, modifier = Modifier.testTag("reply-banner-type")) {
                    Text(stringResource(R.string.type_a_reply), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

/** Paste AI reply: restored with this document's names and saved in the Vault as a reply to it. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ReplySheet(projectName: String?, onRestore: (String) -> Unit, onDismiss: () -> Unit) {
    var reply by rememberSaveable { mutableStateOf("") }
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(24.dp, 0.dp, 24.dp, 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(R.string.paste_ai_reply), style = MaterialTheme.typography.titleLarge, modifier = Modifier.semantics { heading() })
            // Which names come back, as on iOS: its Project's, or its own while Unfiled.
            Text(
                projectName?.let { stringResource(R.string.puts_back_project, it) } ?: stringResource(R.string.puts_back_this_document),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 6,
                overflow = TextOverflow.Ellipsis,
            )
            PasteFromClipboard(onPaste = { reply = it })
            OutlinedTextField(
                value = reply,
                onValueChange = { reply = it },
                label = { Text(stringResource(R.string.ai_reply)) },
                modifier = Modifier.fillMaxWidth().height(220.dp),
            )
            Button({ onRestore(reply) }, Modifier.fillMaxWidth(), enabled = reply.isNotBlank()) {
                Text(stringResource(R.string.restore))
            }
        }
    }
}

/**
 * Notes (#1091 Decision 3): what the user keeps about the document for
 * themselves, saved with it once typing pauses and when the sheet closes.
 * Copy for AI and Share never send them.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun NotesSheet(model: DocumentViewModel) {
    // Null until the notes are read in: typing changes them from then on.
    var notes by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(model) { notes = model.notes() ?: "" }
    ModalBottomSheet(onDismissRequest = model::closeNotes, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(
            Modifier.verticalScroll(rememberScrollState()).padding(24.dp, 0.dp, 24.dp, 24.dp).imePadding(),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    stringResource(R.string.notes),
                    style = MaterialTheme.typography.titleLarge,
                    modifier = Modifier.weight(1f).semantics { heading() },
                )
                TextButton(onClick = model::closeNotes, modifier = Modifier.testTag("notes-done")) { Text(stringResource(R.string.done)) }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.testTag("notes-hint").semantics(mergeDescendants = true) {}) {
                Icon(Icons.Outlined.Lock, contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.size(18.dp))
                Text(
                    stringResource(R.string.notes_private),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            notes?.let { current ->
                OutlinedTextField(
                    value = current,
                    onValueChange = {
                        notes = it
                        model.setNotes(it)
                    },
                    placeholder = { Text(stringResource(R.string.notes_placeholder)) },
                    modifier = Modifier.fillMaxWidth().heightIn(min = 160.dp).testTag("notes-editor"),
                )
            }
        }
    }
}

/** Promote to Document (#1091 Decision 3): the restored reply becomes a new document in the reply's place; the reply stays. */
@Composable
internal fun PromoteButton(onClick: () -> Unit, modifier: Modifier = Modifier) {
    TextButton(onClick = onClick, modifier = modifier.testTag("promote-reply")) { Text(stringResource(R.string.promote_to_document)) }
}

/** The restored reply, its restored values highlighted. */
@Composable
private fun ReplyPane(reply: ReplySnapshot, onPromote: (() -> Unit)?, onRemove: () -> Unit) {
    val restored = reply.restored
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Card {
            Text(
                listOfNotNull(
                    pluralStringResource(R.plurals.values_restored, restored.restoredCount, restored.restoredCount),
                    if (restored.unresolvedCount > 0) stringResource(R.string.unrecognised, restored.unresolvedCount) else null,
                ).joinToString(" · "),
                modifier = Modifier.padding(12.dp),
                style = MaterialTheme.typography.labelLarge,
            )
        }
        RestoredText(restored, reply.styling, Modifier.fillMaxWidth())
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            onPromote?.let { PromoteButton(it) }
            TextButton(onClick = onRemove) { Text(stringResource(R.string.remove_reply)) }
        }
    }
}

/**
 * The share sheet for Copy for AI's text (or a restored reply). Its title
 * and file name come from the core's pseudonymized export name, never the
 * document's own title, which can hold a name the text no longer shows.
 */
fun shareIntent(text: String, fileName: String?, asFile: Boolean, context: Context): Intent {
    // As text: what Ask AI sends too (`AskAi.kt`).
    if (!asFile || fileName == null) return chooser(SharedText.of(text, fileName))
    val title = fileName.removeSuffix(".md")
    val dir = java.io.File(context.cacheDir, "hand-off").apply { deleteRecursively(); mkdirs() }
    val file = java.io.File(dir, fileName).apply { writeText(text) }
    val uri = androidx.core.content.FileProvider.getUriForFile(context, "${context.packageName}.files", file)
    val send = Intent(Intent.ACTION_SEND).setType("text/markdown").putExtra(Intent.EXTRA_STREAM, uri)
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        .putExtra(Intent.EXTRA_TITLE, title)
    return Intent.createChooser(send, title)
}

/**
 * Copies to the clipboard. Text holding originals (a restored reply) is
 * marked sensitive, so Android's clipboard preview hides it (lawkitt/mdoc#321);
 * Copy for AI holds aliases only and stays plain.
 */
internal fun copy(context: Context, text: String, sensitive: Boolean = false) {
    val clip = ClipData.newPlainText("mdoc", text)
    if (sensitive) {
        clip.description.extras = PersistableBundle().apply { putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true) }
    }
    context.getSystemService(ClipboardManager::class.java).setPrimaryClip(clip)
}

/** The way back's words: "From AI reply", or "From a deleted AI reply" once it is gone. */
private val PromotedFrom.label: Int
    get() = when (this) {
        is PromotedFrom.Reply -> R.string.from_ai_reply
        PromotedFrom.DeletedReply -> R.string.from_deleted_ai_reply
    }

private enum class Pane(val label: Int) {
    Text(R.string.text),
    Original(R.string.original),
    Reply(R.string.reply),
}
