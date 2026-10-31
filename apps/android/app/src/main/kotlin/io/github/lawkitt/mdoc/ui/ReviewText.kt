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
import android.widget.TextView
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.viewinterop.AndroidView
import io.github.lawkitt.mdoc.R
import io.github.lawkitt.mdoc.core.Candidate
import io.github.lawkitt.mdoc.core.CandidateId
import io.github.lawkitt.mdoc.core.MentionState
import io.github.lawkitt.mdoc.core.SourceRange
import io.github.lawkitt.mdoc.core.TextOffsets
import uniffi.mdoc_ffi.MarkdownLineKind
import uniffi.mdoc_ffi.MarkdownStyling

/**
 * The document's Markdown source, read-only and selectable, styled from the
 * core's spans with markers dimmed (ADR 0085, 0087, 0089): the twin of the
 * iOS `UITextView`. Every range is the core's UTF-8 source range, mapped
 * once per text; the Markdown is never re-rendered. Candidates are tappable
 * and spoken with their state; a selection offers only Replace…, never Copy,
 * Cut or Share, so originals cannot leave by the clipboard. Nothing is
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
) {
    val colours = ReviewColours.current()
    val tap = rememberUpdatedState(onCandidate)
    val replace = rememberUpdatedState(onReplaceSelection)
    AndroidView(
        modifier = modifier,
        factory = { context -> reviewTextView(context, { tap.value(it) }, { range, text -> replace.value?.invoke(range, text) }) },
        update = { view ->
            // Set only when something drawn changed, so a selection survives recomposition.
            val key = listOf(markdown, styling, candidates, selected, colours).hashCode()
            if (view.tag != key) {
                view.setText(
                    styledReviewText(view.context, markdown, styling, candidates, selected, colours),
                    TextView.BufferType.SPANNABLE,
                )
                view.tag = key
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
    onCandidate: (CandidateId) -> Unit,
    onReplace: (SourceRange, String) -> Unit,
): TextView = TextView(context).apply {
    setTextIsSelectable(true)
    textSize = 17f
    setLineSpacing(4f, 1f)
    // A hyphen inside a name or an email reads as part of it (ADR 0087).
    hyphenationFrequency = Layout.HYPHENATION_FREQUENCY_NONE
    breakStrategy = Layout.BREAK_STRATEGY_SIMPLE
    movementMethod = CandidateMovement(onCandidate)
    customSelectionActionModeCallback = ReplaceOnlyActions(this, onReplace)
    // Long-press opens the selection; nothing to copy is offered (see ReplaceOnlyActions).
    importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
}

/**
 * The selection menu: Replace… and Select all only. Copy, Cut, Share,
 * Translate and the like would carry originals out of mdoc.
 */
private class ReplaceOnlyActions(
    private val view: TextView,
    private val onReplace: (SourceRange, String) -> Unit,
) : ActionMode.Callback {
    override fun onCreateActionMode(mode: ActionMode, menu: Menu): Boolean {
        if (view.getTag(R.id.review_can_replace) == true) {
            menu.add(Menu.NONE, R.id.review_replace, 0, view.context.getString(R.string.replace_ellipsis))
                .setShowAsAction(MenuItem.SHOW_AS_ACTION_ALWAYS)
        }
        return true
    }

    override fun onPrepareActionMode(mode: ActionMode, menu: Menu): Boolean {
        val keep = setOf(R.id.review_replace, android.R.id.selectAll)
        (menu.size() - 1 downTo 0).map(menu::getItem).filter { it.itemId !in keep }.forEach { menu.removeItem(it.itemId) }
        return true
    }

    override fun onActionItemClicked(mode: ActionMode, item: MenuItem): Boolean {
        if (item.itemId != R.id.review_replace) return false
        val offsets = view.getTag(R.id.review_offsets) as? TextOffsets ?: return false
        val start = minOf(view.selectionStart, view.selectionEnd)
        val end = maxOf(view.selectionStart, view.selectionEnd)
        if (end > start) onReplace(offsets.sourceRange(start, end), view.text.substring(start, end))
        mode.finish()
        return true
    }

    override fun onDestroyActionMode(mode: ActionMode) = Unit
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
    // so every offset, stays the same (ADR 0087).
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
