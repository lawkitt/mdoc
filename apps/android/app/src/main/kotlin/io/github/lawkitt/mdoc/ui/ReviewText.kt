package io.github.lawkitt.mdoc.ui

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Typeface
import android.text.Layout
import android.text.Selection
import android.text.Spannable
import android.text.SpannableString
import android.text.Spanned
import android.text.TextPaint
import android.text.method.ArrowKeyMovementMethod
import android.text.style.BackgroundColorSpan
import android.text.style.ClickableSpan
import android.text.style.ForegroundColorSpan
import android.text.style.RelativeSizeSpan
import android.text.style.StrikethroughSpan
import android.text.style.StyleSpan
import android.text.style.TtsSpan
import android.text.style.TypefaceSpan
import android.text.style.UnderlineSpan
import android.view.ActionMode
import android.view.Menu
import android.view.MenuItem
import android.view.MotionEvent
import android.view.View
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.TextView
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.viewinterop.AndroidView
import io.github.lawkitt.mdoc.R
import io.github.lawkitt.mdoc.core.Candidate
import io.github.lawkitt.mdoc.core.CandidateId
import io.github.lawkitt.mdoc.core.MentionState
import io.github.lawkitt.mdoc.core.RestoreResult
import io.github.lawkitt.mdoc.core.RestoredSegment
import io.github.lawkitt.mdoc.core.SelectionAction
import io.github.lawkitt.mdoc.core.SourceRange
import io.github.lawkitt.mdoc.core.TextOffsets
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import uniffi.mdoc_ffi.MarkdownLineKind
import uniffi.mdoc_ffi.MarkdownStyling

/**
 * The document's Markdown source, read-only and selectable, styled from the
 * core's spans with markers dimmed: the twin of the
 * iOS `UITextView`. Every range is the core's UTF-8 source range, mapped
 * once per text; the Markdown is never re-rendered. Candidates are tappable
 * and spoken with their state. A selection is the user's own to copy
 * (#1209): its menu says what it holds, as the core finds it (Copy;
 * Replace N and copy, then Copy with names; or Copy with names alone),
 * beside Replace…; never Cut, Share, Assist or Translate. Nothing is
 * hyphenated.
 */
@Composable
fun ReviewText(
    markdown: String,
    styling: MarkdownStyling?,
    candidates: List<Candidate>,
    selected: CandidateId?,
    onCandidate: (CandidateId) -> Unit,
    onReplaceSelection: ((SourceRange, String) -> Unit)?,
    modifier: Modifier = Modifier,
    /** Where a newly selected candidate's line starts, in pixels from the view's top. */
    onSelectedTop: ((Int) -> Unit)? = null,
    /** A selection's copy menu, from what the core finds in it. */
    selectionMenu: suspend (SourceRange) -> List<SelectionAction> = { SelectionAction.menu(null) },
    /** A hand copy of a selection: the menu's, a keyboard's Ctrl+C or TalkBack's Copy. */
    onCopySelection: (SourceRange, SelectionAction) -> Unit = { _, _ -> },
) {
    val colours = ReviewColours.current()
    val selectedTop = rememberUpdatedState(onSelectedTop)
    val tap = rememberUpdatedState(onCandidate)
    val replace = rememberUpdatedState(onReplaceSelection)
    val menu = rememberUpdatedState(selectionMenu)
    val copy = rememberUpdatedState(onCopySelection)
    val scope = rememberCoroutineScope()
    AndroidView(
        modifier = modifier,
        factory = { context ->
            reviewTextView(
                context, scope, { tap.value(it) }, { range, text -> replace.value?.invoke(range, text) },
                { menu.value(it) }, { range, action -> copy.value(range, action) },
            )
        },
        update = { view ->
            // Set only when something drawn changed, so a selection survives recomposition.
            val key = listOf(markdown, styling, candidates, selected, colours).hashCode()
            if (view.tag != key) {
                view.setText(
                    styledReviewText(view.context, markdown, styling, candidates, selected, colours) { tap.value(it) },
                    TextView.BufferType.SPANNABLE,
                )
                view.tag = key
            }
            // A candidate opened from elsewhere (Next proposal, Identities) is
            // scrolled to: report its line once laid out.
            if (selected != null && view.getTag(R.id.review_selected) != selected.raw) {
                view.setTag(R.id.review_selected, selected.raw)
                view.post {
                    val text = view.text as? Spanned ?: return@post
                    val span = text.getSpans(0, text.length, CandidateSpan::class.java).firstOrNull { it.id == selected } ?: return@post
                    val layout = view.layout ?: return@post
                    val top = layout.getLineTop(layout.getLineForOffset(text.getSpanStart(span))) + view.totalPaddingTop
                    selectedTop.value?.invoke(top)
                }
            } else if (selected == null) {
                view.setTag(R.id.review_selected, null)
            }
            view.setTextColor(colours.text)
            view.setTag(R.id.review_offsets, TextOffsets(markdown))
            view.setTag(R.id.review_can_replace, onReplaceSelection != null)
        },
    )
}

/** The colours the review text draws with, from the Material theme. */
data class ReviewColours(val text: Int, val marker: Int, val secondary: Int, val link: Int, val codeFill: Int, val dark: Boolean) {
    companion object {
        @Composable
        fun current(): ReviewColours {
            val scheme = MaterialTheme.colorScheme
            val dark = scheme.background.luminance() < 0.5f
            return ReviewColours(
                text = scheme.onSurface.toArgb(),
                marker = scheme.onSurface.copy(alpha = 0.38f).toArgb(),
                secondary = scheme.onSurfaceVariant.toArgb(),
                link = scheme.primary.toArgb(),
                codeFill = scheme.surfaceVariant.toArgb(),
                dark = dark,
            )
        }

        private fun Color.luminance(): Float = 0.2126f * red + 0.7152f * green + 0.0722f * blue
    }
}

@SuppressLint("ClickableViewAccessibility")
private fun reviewTextView(
    context: Context,
    scope: CoroutineScope,
    onCandidate: (CandidateId) -> Unit,
    onReplace: (SourceRange, String) -> Unit,
    menuFor: suspend (SourceRange) -> List<SelectionAction>,
    onCopy: (SourceRange, SelectionAction) -> Unit,
): TextView = SelectionTextView(context).apply {
    setTextIsSelectable(true)
    textSize = 17f
    setLineSpacing(4f, 1f)
    // A hyphen inside a name or an email reads as part of it.
    hyphenationFrequency = Layout.HYPHENATION_FREQUENCY_NONE
    breakStrategy = Layout.BREAK_STRATEGY_SIMPLE
    movementMethod = CandidateMovement(onCandidate)
    customSelectionActionModeCallback = SelectionActions(this, scope, menuFor, onCopy, onReplace)
    // As written, marked unless the core finds the text clean.
    onCopyAsWritten = { range -> onCopy(range, SelectionAction.CopyWithNames) }
    importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
}

/** The selection as the core's source range; null when nothing is selected. */
internal fun TextView.selectedRange(): SourceRange? {
    val offsets = getTag(R.id.review_offsets) as? TextOffsets ?: return null
    val start = minOf(selectionStart, selectionEnd)
    val end = maxOf(selectionStart, selectionEnd)
    return if (start >= 0 && end > start) offsets.sourceRange(start, end) else null
}

/**
 * The review text's view. A copy of the selection is the user's own
 * (#1209): a keyboard's Ctrl+C and TalkBack's Copy hand it to
 * [onCopyAsWritten], which copies it as written and marks it unless the
 * core finds it clean, never as an unmarked system copy. Cut, Paste, Share
 * and Assist are refused however they are asked for: they would change
 * the text or send it on without saying so.
 */
class SelectionTextView(context: Context) : TextView(context) {
    /** A copy of the selection as written. */
    var onCopyAsWritten: ((SourceRange) -> Unit)? = null

    override fun onTextContextMenuItem(id: Int): Boolean = when (id) {
        android.R.id.copy -> {
            copyAsWritten()
            true
        }
        in BLOCKED -> true
        else -> super.onTextContextMenuItem(id)
    }

    /** Copies the selection, if any; whether there was one. */
    private fun copyAsWritten(): Boolean {
        val range = selectedRange() ?: return false
        onCopyAsWritten?.invoke(range)
        return true
    }

    override fun onInitializeAccessibilityNodeInfo(info: AccessibilityNodeInfo) {
        super.onInitializeAccessibilityNodeInfo(info)
        info.removeAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_CUT)
        info.removeAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_PASTE)
    }

    override fun performAccessibilityAction(action: Int, arguments: android.os.Bundle?): Boolean = when (action) {
        AccessibilityNodeInfo.ACTION_COPY -> copyAsWritten()
        AccessibilityNodeInfo.ACTION_CUT, AccessibilityNodeInfo.ACTION_PASTE -> false
        else -> super.performAccessibilityAction(action, arguments)
    }

    private companion object {
        val BLOCKED = setOf(
            android.R.id.cut, android.R.id.paste, android.R.id.pasteAsPlainText,
            android.R.id.shareText, android.R.id.textAssist,
        )
    }
}

/**
 * The selection menu: its copy items, as the core finds the selection
 * (asked off the main thread, and again whenever the selection moves),
 * then Replace… and Select all. The system's own Copy, Cut, Share, Assist,
 * Translate and the like are removed: they would copy unmarked or send
 * the text on.
 */
internal class SelectionActions(
    private val view: TextView,
    private val scope: CoroutineScope,
    private val menuFor: suspend (SourceRange) -> List<SelectionAction>,
    private val onCopy: (SourceRange, SelectionAction) -> Unit,
    private val onReplace: (SourceRange, String) -> Unit,
) : ActionMode.Callback {
    /** The selection the copy items were found for, and them. */
    private var found: Pair<SourceRange, List<SelectionAction>>? = null

    /** The check running, and the selection it is for. */
    private var checking: Pair<SourceRange, Job>? = null

    override fun onCreateActionMode(mode: ActionMode, menu: Menu): Boolean {
        if (view.getTag(R.id.review_can_replace) == true) {
            menu.add(Menu.NONE, R.id.review_replace, REPLACE_ORDER, view.context.getString(R.string.replace_ellipsis))
                .setShowAsAction(MenuItem.SHOW_AS_ACTION_ALWAYS)
        }
        return true
    }

    override fun onPrepareActionMode(mode: ActionMode, menu: Menu): Boolean {
        val keep = setOf(R.id.review_replace, android.R.id.selectAll)
        (menu.size() - 1 downTo 0).map(menu::getItem).filter { it.itemId !in keep }.forEach { menu.removeItem(it.itemId) }
        val range = view.selectedRange() ?: return true
        val actions = found?.takeIf { it.first == range }?.second
        if (actions == null) {
            if (checking?.first != range) ask(mode, range)
            return true
        }
        actions.forEachIndexed { order, action ->
            menu.add(Menu.NONE, action.itemId, order, action.title(view.context)).setShowAsAction(MenuItem.SHOW_AS_ACTION_ALWAYS)
        }
        return true
    }

    /** Asks the core what `range` holds; its items show once it answers, while `range` is still selected. */
    private fun ask(mode: ActionMode, range: SourceRange) {
        checking?.second?.cancel()
        val job = scope.launch {
            val actions = menuFor(range)
            found = range to actions
            checking = null
            if (view.selectedRange() == range) mode.invalidate()
        }
        checking = range to job
    }

    override fun onActionItemClicked(mode: ActionMode, item: MenuItem): Boolean {
        val range = view.selectedRange()
        if (item.itemId == R.id.review_replace) {
            val offsets = view.getTag(R.id.review_offsets) as? TextOffsets ?: return false
            if (range != null) onReplace(range, view.text.substring(offsets.utf16(range.start.toLong()), offsets.utf16(range.end.toLong())))
            mode.finish()
            return true
        }
        val (at, actions) = found ?: return false
        val action = actions.firstOrNull { it.itemId == item.itemId } ?: return false
        if (range == at) onCopy(at, action)
        mode.finish()
        return true
    }

    override fun onDestroyActionMode(mode: ActionMode) {
        checking?.second?.cancel()
        checking = null
        found = null
    }

    private companion object {
        /** After the copy items. */
        const val REPLACE_ORDER = 10

        val SelectionAction.itemId: Int
            get() = when (this) {
                SelectionAction.Copy -> R.id.review_copy
                is SelectionAction.ReplaceAndCopy -> R.id.review_replace_and_copy
                SelectionAction.CopyWithNames -> R.id.review_copy_with_names
            }

        fun SelectionAction.title(context: Context): String = when (this) {
            SelectionAction.Copy -> context.getString(R.string.copy)
            is SelectionAction.ReplaceAndCopy -> context.getString(R.string.selection_replace_and_copy, count)
            SelectionAction.CopyWithNames -> context.getString(R.string.selection_copy_with_names)
        }
    }
}

/** Selection as usual, and a tap on a candidate opens it. */
private class CandidateMovement(private val onCandidate: (CandidateId) -> Unit) : ArrowKeyMovementMethod() {
    override fun onTouchEvent(widget: TextView, buffer: Spannable, event: MotionEvent): Boolean {
        if (event.action == MotionEvent.ACTION_UP && Selection.getSelectionStart(buffer) == Selection.getSelectionEnd(buffer)) {
            val layout = widget.layout
            if (layout != null) {
                val x = event.x.toInt() - widget.totalPaddingLeft + widget.scrollX
                val y = event.y.toInt() - widget.totalPaddingTop + widget.scrollY
                val line = layout.getLineForVertical(y)
                val offset = layout.getOffsetForHorizontal(line, x.toFloat())
                val span = buffer.getSpans(offset, offset, CandidateSpan::class.java).firstOrNull()
                if (span != null && x <= layout.getLineRight(line)) {
                    onCandidate(span.id)
                    return true
                }
            }
        }
        return super.onTouchEvent(widget, buffer, event)
    }
}

/** A candidate: its category's fill, an underline, and the tap target. */
class CandidateSpan(val id: CandidateId, private val onTap: (CandidateId) -> Unit) : ClickableSpan() {
    override fun onClick(widget: View) = onTap(id)

    override fun updateDrawState(paint: TextPaint) {
        paint.isUnderlineText = true
    }
}

/** Builds the styled text; public for tests. */
fun styledReviewText(
    context: Context,
    markdown: String,
    styling: MarkdownStyling?,
    candidates: List<Candidate>,
    selected: CandidateId?,
    colours: ReviewColours,
    onTap: (CandidateId) -> Unit = {},
): SpannableString {
    val text = SpannableString(markdown)
    val offsets = TextOffsets(markdown)
    val flags = Spanned.SPAN_EXCLUSIVE_EXCLUSIVE
    fun clamp(start: ULong, end: ULong): IntRange? {
        val from = offsets.utf16(start).coerceIn(0, markdown.length)
        val to = offsets.utf16(end).coerceIn(0, markdown.length)
        return if (to > from) from until to else null
    }
    fun set(span: Any, range: IntRange) = text.setSpan(span, range.first, range.last + 1, flags)

    // A blank source line draws about a third of a line high; the text, and
    // so every offset, stays the same.
    var lineStart = 0
    while (lineStart < markdown.length) {
        val newline = markdown.indexOf('\n', lineStart).let { if (it < 0) markdown.length else it }
        if (markdown.substring(lineStart, newline).isBlank() && newline < markdown.length) {
            set(RelativeSizeSpan(0.33f), lineStart..newline)
        }
        lineStart = newline + 1
    }

    styling?.lines?.forEach { line ->
        val range = clamp(line.range.start, line.range.end) ?: return@forEach
        when (val kind = line.kind) {
            is MarkdownLineKind.Heading -> {
                set(RelativeSizeSpan(if (kind.level.toInt() == 1) 1.35f else if (kind.level.toInt() == 2) 1.2f else 1.08f), range)
                set(StyleSpan(Typeface.BOLD), range)
            }
            MarkdownLineKind.Quote -> set(ForegroundColorSpan(colours.secondary), range)
            MarkdownLineKind.CodeBlock -> set(TypefaceSpan("monospace"), range)
            // Table rows smaller and monospaced, so columns line up and wrap less.
            MarkdownLineKind.TableHeader -> {
                set(TypefaceSpan("monospace"), range)
                set(RelativeSizeSpan(0.85f), range)
                set(StyleSpan(Typeface.BOLD), range)
            }
            MarkdownLineKind.TableRow -> {
                set(TypefaceSpan("monospace"), range)
                set(RelativeSizeSpan(0.85f), range)
            }
            MarkdownLineKind.Rule -> Unit
        }
    }

    styling?.spans?.filter { !it.marker }?.forEach { span ->
        val range = clamp(span.range.start, span.range.end) ?: return@forEach
        if (span.strong) set(StyleSpan(Typeface.BOLD), range)
        if (span.emphasis) set(StyleSpan(Typeface.ITALIC), range)
        if (span.code) {
            set(TypefaceSpan("monospace"), range)
            set(BackgroundColorSpan(colours.codeFill), range)
        }
        if (span.strikethrough) set(StrikethroughSpan(), range)
        if (span.underline) set(UnderlineSpan(), range)
        if (span.link) set(ForegroundColorSpan(colours.link), range)
        if (span.highlight) set(BackgroundColorSpan(Palette.highlight(colours.dark)), range)
        // The document's own colours, as the desktop draws them (0xRRGGBBAA).
        span.color?.let { set(ForegroundColorSpan(rgba(it)), range) }
        span.background?.let { set(BackgroundColorSpan(rgba(it)), range) }
    }

    // Markers last, so they stay dimmed whatever they sit in.
    styling?.spans?.filter { it.marker }?.forEach { span ->
        val range = clamp(span.range.start, span.range.end) ?: return@forEach
        set(ForegroundColorSpan(colours.marker), range)
    }

    for (candidate in candidates) {
        val range = clamp(candidate.range.start.toULong(), candidate.range.end.toULong()) ?: continue
        val colour = Palette.category(candidate.category, colours.dark)
        val fill = Palette.fill(colours.dark)
        val label = context.getString(candidate.category.label)
        val spoken = when (candidate.state) {
            MentionState.Proposed -> context.getString(R.string.spoken_proposed, label, candidate.alias)
            MentionState.Replaced -> context.getString(R.string.spoken_replaced, label, candidate.original)
            MentionState.Kept -> context.getString(R.string.spoken_kept, label)
        }
        set(CandidateSpan(candidate.id, onTap), range)
        // What TalkBack says: the words, then the category and state.
        set(TtsSpan.TextBuilder("${text.substring(range.first, range.last + 1)}, $spoken").build(), range)
        when (candidate.state) {
            MentionState.Proposed -> set(BackgroundColorSpan(withAlpha(colour, fill.proposed)), range)
            MentionState.Replaced -> {
                set(BackgroundColorSpan(withAlpha(colour, fill.replaced)), range)
                set(TypefaceSpan("monospace"), range)
            }
            MentionState.Kept -> set(ForegroundColorSpan(colours.secondary), range)
        }
        if (candidate.id == selected) set(BackgroundColorSpan(withAlpha(colour, fill.selected)), range)
    }
    return text
}

private fun rgba(value: UInt): Int {
    val v = value.toLong()
    return (((v and 0xFF) shl 24) or ((v shr 8) and 0xFFFFFF)).toInt()
}

private fun withAlpha(colour: Int, alpha: Float): Int = (colour and 0x00FFFFFF) or ((alpha * 255).toInt() shl 24)

/**
 * A restored reply, styled with the review text's spans and colours, its
 * restored values highlighted. The reply holds the user's own
 * names again, so it is selectable and copyable as ordinary text.
 */
@Composable
fun RestoredText(result: RestoreResult, styling: MarkdownStyling?, modifier: Modifier = Modifier) {
    val colours = ReviewColours.current()
    val restoredFill = MaterialTheme.colorScheme.tertiaryContainer.toArgb()
    val unresolvedFill = MaterialTheme.colorScheme.errorContainer.toArgb()
    AndroidView(
        modifier = modifier,
        factory = { context ->
            TextView(context).apply {
                setTextIsSelectable(true)
                textSize = 17f
                setLineSpacing(4f, 1f)
                hyphenationFrequency = Layout.HYPHENATION_FREQUENCY_NONE
                breakStrategy = Layout.BREAK_STRATEGY_SIMPLE
            }
        },
        update = { view ->
            val key = listOf(result, styling, colours).hashCode()
            if (view.tag == key) return@AndroidView
            val markdown = result.text
            val text = styledReviewText(view.context, markdown, styling, emptyList(), null, colours)
            var at = 0
            for (segment in result.segments) {
                val piece = when (segment) {
                    is RestoredSegment.Text -> segment.text
                    is RestoredSegment.Restored -> segment.value
                    is RestoredSegment.Unresolved -> segment.token
                }
                val fill = when (segment) {
                    is RestoredSegment.Restored -> restoredFill
                    is RestoredSegment.Unresolved -> unresolvedFill
                    is RestoredSegment.Text -> null
                }
                if (fill != null && piece.isNotEmpty()) {
                    text.setSpan(BackgroundColorSpan(fill), at, at + piece.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                }
                at += piece.length
            }
            view.setText(text, TextView.BufferType.SPANNABLE)
            view.setTextColor(colours.text)
            view.tag = key
        },
    )
}
