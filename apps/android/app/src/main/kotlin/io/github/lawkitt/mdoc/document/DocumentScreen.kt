package io.github.lawkitt.mdoc.document

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.PersistableBundle
import androidx.compose.foundation.background
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
import androidx.compose.foundation.layout.heightIn
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
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Person
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
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.lawkitt.mdoc.R
import io.github.lawkitt.mdoc.core.Candidate
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.Decision
import io.github.lawkitt.mdoc.core.DocumentPhase
import io.github.lawkitt.mdoc.core.Identity
import io.github.lawkitt.mdoc.core.MentionState
import io.github.lawkitt.mdoc.core.OriginalSource
import io.github.lawkitt.mdoc.core.ReplySnapshot
import io.github.lawkitt.mdoc.core.RestoredSegment
import io.github.lawkitt.mdoc.core.Scope
import io.github.lawkitt.mdoc.home.PasteFromClipboard
import io.github.lawkitt.mdoc.jobs.JobKind
import io.github.lawkitt.mdoc.setup.SetupCard
import io.github.lawkitt.mdoc.setup.SetupController
import io.github.lawkitt.mdoc.ui.OriginalView
import io.github.lawkitt.mdoc.ui.Palette
import io.github.lawkitt.mdoc.ui.RestoredText
import io.github.lawkitt.mdoc.ui.ReviewText
import kotlinx.coroutines.launch
import uniffi.mdoc_ffi.JobStage

/**
 * One document: its text with candidates, the primary action that follows
 * its state, and the hand-off bar once nothing is left to decide (ADR 0085,
 * 0087). The twin of the iOS `DocumentScreen`.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DocumentScreen(
    model: DocumentViewModel,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    beforeJob: () -> Unit = {},
    setup: SetupController? = null,
) {
    val state by model.state.collectAsStateWithLifecycle()
    val job by model.running.collectAsStateWithLifecycle()
    val interrupted by model.interrupted.collectAsStateWithLifecycle()
    val snapshot = state.snapshot
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val snackbar = remember { SnackbarHostState() }
    var menu by remember { mutableStateOf(false) }
    var showsOriginal by rememberSaveable { mutableStateOf(false) }

    LaunchedEffect(state.notice) {
        state.notice?.let {
            snackbar.showSnackbar(context.getString(it))
            model.noticeShown()
        }
    }
    // The job's result arrives as a new snapshot when it ends.
    LaunchedEffect(job) { if (job == null) model.refresh() }

    Scaffold(
        modifier = modifier,
        snackbarHost = { SnackbarHost(snackbar) },
        topBar = {
            TopAppBar(
                title = { Text(snapshot.title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
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
                        // The hand-off's second way out (ADR 0087's long press on iOS).
                        if (model.primaryAction(state, job) == PrimaryAction.HandOff && !state.showsReply) {
                            DropdownMenuItem(
                                text = { Text(stringResource(R.string.share_as_file)) },
                                onClick = {
                                    menu = false
                                    scope.launch {
                                        model.handOff()?.let { context.startActivity(shareIntent(it.text, it.fileName, true, context)) }
                                    }
                                },
                            )
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
            ActionBar(
                action = model.primaryAction(state, job),
                hasReply = snapshot.reply != null,
                showsReply = state.showsReply,
                onRecognize = {
                    beforeJob()
                    model.recognize(context.getString(R.string.recognizing_title, snapshot.title))
                },
                onFindNames = {
                    beforeJob()
                    model.findNames(context.getString(R.string.finding_names_in, snapshot.title))
                },
                onCancel = model::cancelJob,
                onReplaceAll = model::replaceAll,
                onShare = { asFile ->
                    scope.launch { model.handOff()?.let { context.startActivity(shareIntent(it.text, it.fileName, asFile, context)) } }
                },
                onCopy = {
                    scope.launch {
                        model.handOff()?.let {
                            copy(context, it.text)
                            model.notice(R.string.copied_for_ai)
                        }
                    }
                },
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
        },
    ) { padding ->
        BoxWithConstraints(Modifier.padding(padding).fillMaxSize()) {
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
            if (!model.findsNames && snapshot.phase == DocumentPhase.Unscanned) Notice(stringResource(R.string.below_floor_note))
            snapshot.warning?.let { Notice(it) }
            val needs = state.needsSetup
            if (needs != null && setup != null) {
                val setupState by setup.state.collectAsStateWithLifecycle()
                // Download & continue: the job that asked for setup runs again (ADR 0085).
                SetupCard(
                    state = setupState,
                    continues = true,
                    onDownload = {
                        beforeJob()
                        setup.download {
                            if (needs == JobKind.Scan) {
                                model.findNames(context.getString(R.string.finding_names_in, snapshot.title))
                            } else {
                                model.recognize(context.getString(R.string.recognizing_title, snapshot.title))
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
            // screen the original sits beside the text (ADR 0085).
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
                    RecognizingPane(
                        stringResource(
                            if (!snapshot.kind.needsRecognition) R.string.import_not_finished_body else R.string.status_not_recognized,
                        ),
                        paneModifier,
                    )
                } else {
                    ReviewText(
                        markdown = snapshot.markdown,
                        styling = snapshot.styling,
                        candidates = snapshot.candidates,
                        selected = state.reviewing,
                        onCandidate = model::open,
                        onReplaceSelection = model::startManualReplace,
                        modifier = paneModifier.verticalScroll(rememberScrollState()).padding(16.dp),
                    )
                }
            }
            Box(Modifier.weight(1f).fillMaxWidth()) {
                when {
                    pane == Pane.Reply -> Readable { ReplyPane(snapshot.reply!!, onRemove = model::clearReply) }
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
        IdentitiesSheet(snapshot.identities, onDismiss = { model.showIdentities(false) })
    }
    if (state.showsReplySheet) {
        ReplySheet(snapshot.caseName, onRestore = model::restoreReply, onDismiss = { model.showReplySheet(false) })
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
private fun RecognizingPane(text: String, modifier: Modifier = Modifier.fillMaxSize()) {
    Column(modifier.padding(32.dp), verticalArrangement = Arrangement.Center) {
        Text(text, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** The bar under the document: its one primary action, or the hand-off once nothing is left to decide. */
@Composable
private fun ActionBar(
    action: PrimaryAction,
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
                    Text(stageLabel(action.job.kind, action.job.stage), style = MaterialTheme.typography.bodyMedium)
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                    OutlinedButton(onClick = onCancel, Modifier.fillMaxWidth()) { Text(stringResource(R.string.cancel)) }
                }
                PrimaryAction.Waiting -> Text(stringResource(R.string.waiting_for_other), style = MaterialTheme.typography.bodyMedium)
                is PrimaryAction.ReplaceAll -> Primary(stringResource(R.string.replace_count, action.count), onReplaceAll)
                // On the Reply tab the bar acts on the reply (ADR 0087); TalkBack
                // hears the full names, the buttons show the short ones.
                PrimaryAction.HandOff -> if (showsReply) {
                    HandOffRow(
                        Triple(onShareReply, R.string.share, R.string.share_reply),
                        Triple(onCopyReply, R.string.copy, R.string.copy_reply),
                        Triple(onPasteReply, R.string.paste, R.string.paste_another_reply),
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
 * line, as at the largest text sizes (ADR 0087). Each is (action, label,
 * what TalkBack reads).
 */
@Composable
private fun HandOffRow(vararg buttons: Triple<() -> Unit, Int, Int>) {
    val stacked = LocalDensity.current.fontScale > 1.3f
    val items: @Composable (Modifier) -> Unit = { modifier ->
        buttons.forEachIndexed { index, (onClick, label, spoken) ->
            val name = stringResource(spoken)
            val described = modifier.semantics { contentDescription = name }
            val text: @Composable () -> Unit = { Text(stringResource(label), maxLines = 1, overflow = TextOverflow.Ellipsis) }
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
private fun stageLabel(kind: JobKind, stage: JobStage?): String = stringResource(
    when (stage) {
        JobStage.READING -> R.string.reading_the_file
        JobStage.CONVERTING -> R.string.converting
        JobStage.RECOGNIZING -> R.string.recognizing_text
        JobStage.SCANNING -> R.string.finding_names
        null -> if (kind == JobKind.Scan) R.string.finding_names else R.string.reading_the_file
    },
)

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
            // Side by side, or stacked when the labels would not fit (ADR 0087).
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
private fun IdentitiesSheet(identities: List<Identity>, onDismiss: () -> Unit) {
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
                    ListItem(
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
                }
            }
        }
    }
}

/** Paste AI reply: restored with this document's case and kept as its reply (ADR 0078). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ReplySheet(caseName: String?, onRestore: (String) -> Unit, onDismiss: () -> Unit) {
    var reply by rememberSaveable { mutableStateOf("") }
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(24.dp, 0.dp, 24.dp, 24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(R.string.paste_ai_reply), style = MaterialTheme.typography.titleLarge, modifier = Modifier.semantics { heading() })
            // Which names come back, as on iOS: the case's, never another's.
            Text(
                caseName?.let { stringResource(R.string.puts_back_case, it) } ?: stringResource(R.string.puts_back_this_case),
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

/** The restored reply, its restored values highlighted (ADR 0087). */
@Composable
private fun ReplyPane(reply: ReplySnapshot, onRemove: () -> Unit) {
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
        TextButton(onClick = onRemove) { Text(stringResource(R.string.remove_reply)) }
    }
}

/**
 * The share sheet for Copy for AI's text (or a restored reply). Its title
 * and file name come from the core's pseudonymized export name, never the
 * document's own title, which can hold a name the text no longer shows.
 */
fun shareIntent(text: String, fileName: String?, asFile: Boolean, context: Context): Intent {
    val title = fileName?.removeSuffix(".md")
    val send = if (asFile && fileName != null) {
        val dir = java.io.File(context.cacheDir, "hand-off").apply { deleteRecursively(); mkdirs() }
        val file = java.io.File(dir, fileName).apply { writeText(text) }
        val uri = androidx.core.content.FileProvider.getUriForFile(context, "${context.packageName}.files", file)
        Intent(Intent.ACTION_SEND).setType("text/markdown").putExtra(Intent.EXTRA_STREAM, uri)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    } else {
        Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text)
    }
    title?.let { send.putExtra(Intent.EXTRA_TITLE, it) }
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

private enum class Pane(val label: Int) {
    Text(R.string.text),
    Original(R.string.original),
    Reply(R.string.reply),
}
