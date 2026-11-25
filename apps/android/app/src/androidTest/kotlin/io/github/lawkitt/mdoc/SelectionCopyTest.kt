package io.github.lawkitt.mdoc

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.text.Selection
import android.text.Spannable
import android.view.ActionMode
import android.view.Menu
import android.view.MenuInflater
import android.view.View
import android.view.ViewGroup
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.PopupMenu
import androidx.compose.ui.test.junit4.AndroidComposeTestRule
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.rules.ActivityScenarioRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.runner.lifecycle.ActivityLifecycleMonitorRegistry
import androidx.test.runner.lifecycle.Stage
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.Documents
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.SourceRange
import io.github.lawkitt.mdoc.ui.SelectionTextView
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.Candidate
import uniffi.mdoc_ffi.JobStage
import uniffi.mdoc_ffi.TextRange

/**
 * A selection copied by hand on the document screen (#1209; Vault &
 * Projects item 42), in place of the review text's old refusals: the
 * selection menu's three states (Copy; Replace N and copy, then Copy
 * with names; Copy with names alone), each copy's marking (sensitive
 * unless the core finds the selection clean), a keyboard's Ctrl+C, and
 * Share and Assist never offered. The menu is the screen's own review
 * text and selection callback, driven with a real menu, since the
 * floating toolbar itself is the system's. Below the detector floor, as
 * the CI emulator is; [SelectionCopyDetectorTierTest] has a document
 * never scanned.
 */
@RunWith(AndroidJUnit4::class)
class SelectionCopyTest {
    @get:Rule(order = 0)
    val core = TestCore()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    private val app get() = core.app

    /** Done when: a clean selection offers Copy, which copies it as written and unmarked; Ctrl+C alike. */
    @Test
    fun aCleanSelectionOffersCopyUnmarked() {
        val letter = compose.ready(app.core.documents)
        compose.openDocument(letter.snapshot().title)
        val alias = letter.snapshot().candidates.single().alias
        val part = "$alias, your claim"

        val menu = compose.selectionMenu(part)
        assertEquals(listOf("Copy", "Replace…", "Select all"), menu.titles())
        compose.tapMenu(menu, "Copy")
        compose.waitForClip(part)
        assertFalse("a clean copy is ordinary", sensitive())

        stale()
        compose.onReviewText { view ->
            view.select(part)
            view.onTextContextMenuItem(android.R.id.copy)
        }
        compose.waitForClip(part)
        assertFalse(sensitive())
    }

    /**
     * Done when: a proposal in the selection offers Replace 1 and copy,
     * then Copy with names; Copy with names copies the name as written,
     * marked; Replace 1 and copy replaces it and copies the alias, clean.
     */
    @Test
    fun namesNotReplacedOfferReplaceAndCopy() {
        val note = compose.note(app.core.documents)
        compose.openDocument(note.snapshot().title)
        val part = "Write to $EMAIL"

        val menu = compose.selectionMenu(part)
        assertEquals(listOf("Replace 1 and copy", "Copy with names", "Replace…", "Select all"), menu.titles())
        compose.tapMenu(menu, "Copy with names")
        compose.waitForClip(part)
        assertTrue("a copy with names is marked", sensitive())
        assertEquals("nothing replaced", 1, note.snapshot().pendingCount)

        stale()
        compose.tapMenu(compose.selectionMenu(part), "Replace 1 and copy")
        compose.waitUntil(10_000) { note.snapshot().pendingCount == 0 }
        val alias = note.snapshot().candidates.single().alias
        compose.waitForClip("Write to $alias")
        assertFalse("replaced, the copy is clean", sensitive())
        assertEquals(0, note.snapshot().pendingCount)
    }

    /**
     * Done when: a keyboard's Ctrl+C and TalkBack's Copy copy as written,
     * marked unless clean; Share and Assist are never listed nor acted on,
     * and the system's own unmarked Copy is gone from the menu.
     */
    @Test
    fun ctrlCCopiesAsWrittenAndShareStaysOff() {
        val note = compose.note(app.core.documents)
        compose.openDocument(note.snapshot().title)
        val part = "Write to $EMAIL"

        stale()
        compose.onReviewText { view ->
            view.select(part)
            view.onTextContextMenuItem(android.R.id.copy)
        }
        compose.waitForClip(part)
        assertTrue(sensitive())

        stale()
        compose.onReviewText { view ->
            view.select(part)
            assertTrue(view.onTextContextMenuItem(android.R.id.shareText))
            assertTrue(view.onTextContextMenuItem(android.R.id.textAssist))
            assertTrue(view.onTextContextMenuItem(android.R.id.cut))
        }
        compose.waitForIdle()
        assertEquals("Share, Assist and Cut do nothing", STALE, clip())
        assertEquals("the text is unchanged", note.snapshot().markdown, compose.reviewText())

        compose.onReviewText { view ->
            view.select(part)
            assertTrue(view.performAccessibilityAction(AccessibilityNodeInfo.ACTION_COPY, null))
            assertFalse(view.performAccessibilityAction(AccessibilityNodeInfo.ACTION_CUT, null))
        }
        compose.waitForClip(part)
        assertTrue(sensitive())

        val menu = compose.selectionMenu(part)
        val ids = compose.onMain { (0 until menu.menu.size()).map { menu.menu.getItem(it).itemId } }
        for (gone in listOf(android.R.id.copy, android.R.id.shareText, android.R.id.textAssist, android.R.id.cut, PROCESS_TEXT)) {
            assertFalse("$gone listed", gone in ids)
        }
    }

    /** Review: a mouse's right-click menu on a selection offers Copy, marked unless clean, and Select all only. */
    @Test
    fun aRightClickOffersCopyAndSelectAllOnly() {
        val note = compose.note(app.core.documents)
        compose.openDocument(note.snapshot().title)
        val part = "Write to $EMAIL"
        compose.waitUntil(15_000) { compose.reviewText().contains(part) }

        var ids: List<Int>? = null
        compose.onReviewText { view ->
            view.select(part)
            // Listeners see the menu after the view built it.
            view.setOnCreateContextMenuListener { menu, _, _ -> ids = (0 until menu.size()).map { menu.getItem(it).itemId } }
            val (x, y) = view.pointIn(part)
            view.showContextMenu(x, y)
        }
        compose.waitUntil(5_000) { compose.onMain { ids != null } }
        compose.back()
        compose.onReviewText { it.setOnCreateContextMenuListener(null) }
        val listed = compose.onMain { ids!! }
        assertTrue("$listed", android.R.id.copy in listed)
        for (gone in listOf(android.R.id.shareText, android.R.id.textAssist, android.R.id.cut, android.R.id.paste, android.R.id.replaceText)) {
            assertFalse("$gone listed", gone in listed)
        }

        stale()
        compose.onReviewText { view ->
            view.select(part)
            view.onTextContextMenuItem(android.R.id.copy)
        }
        compose.waitForClip(part)
        assertTrue(sensitive())
    }
}

/** A document never scanned, at the detector tier: Copy with names alone, marked. */
@RunWith(AndroidJUnit4::class)
class SelectionCopyDetectorTierTest {
    /** 16 GB: the detector tier, where a document is scanned by Find names. */
    @get:Rule(order = 0)
    val core = TestCore(physicalMemory = 16L shl 30)

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    /** Done when: a document never scanned offers Copy with names alone, and its copy is marked. */
    @Test
    fun neverScannedOffersCopyWithNamesAlone() {
        val text = "Ann Lee signed the lease."
        val memo = core.app.core.documents.importDocument(ImportSource.Text(text), "Memo ${System.nanoTime()}")
        memo.recognize(QuietJobs)
        compose.recreate()
        compose.openDocument(memo.snapshot().title)

        val menu = compose.selectionMenu("Ann Lee signed")
        assertEquals(listOf("Copy with names", "Replace…", "Select all"), menu.titles())
        stale()
        compose.tapMenu(menu, "Copy with names")
        compose.waitForClip("Ann Lee signed")
        assertTrue(sensitive())
    }
}

// The screen's review text and its selection menu

private typealias SelectionRule = AndroidComposeTestRule<ActivityScenarioRule<MainActivity>, MainActivity>

private const val EMAIL = "ann@example.com"
private const val STALE = "before"

/** A Translate or Search item: the system's text-processing apps, which the menu drops. */
private val PROCESS_TEXT = View.generateViewId()

private object QuietJobs : JobObserver {
    override fun stage(stage: JobStage) = Unit

    override fun onCancel(handler: () -> Unit) = Unit
}

/** The selection menu as the system builds it, then as the review text's callback leaves it. */
private class OpenMenu(val menu: Menu, val mode: ActionMode, val callback: ActionMode.Callback)

/** An action mode that prepares its menu again when asked, as the system's does. */
private class TestMode(private val view: View, private val menu: Menu) : ActionMode() {
    lateinit var callback: ActionMode.Callback

    override fun invalidate() {
        callback.onPrepareActionMode(this, menu)
    }

    override fun finish() = Unit

    override fun getMenu(): Menu = menu

    override fun setTitle(title: CharSequence?) = Unit

    override fun setTitle(resId: Int) = Unit

    override fun setSubtitle(subtitle: CharSequence?) = Unit

    override fun setSubtitle(resId: Int) = Unit

    override fun setCustomView(view: View?) = Unit

    override fun getTitle(): CharSequence? = null

    override fun getSubtitle(): CharSequence? = null

    override fun getCustomView(): View? = null

    override fun getMenuInflater(): MenuInflater = MenuInflater(view.context)
}

private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

private fun <T> SelectionRule.onMain(body: () -> T): T {
    var result: Result<T>? = null
    instrumentation.runOnMainSync { result = runCatching(body) }
    return result!!.getOrThrow()
}

private fun find(view: View): SelectionTextView? = when (view) {
    is SelectionTextView -> view
    is ViewGroup -> (0 until view.childCount).firstNotNullOfOrNull { find(view.getChildAt(it)) }
    else -> null
}

/** `body` with the document screen's review text, on the main thread. */
private fun <T> SelectionRule.onReviewText(body: (SelectionTextView) -> T): T {
    waitUntil(15_000) {
        onMain {
            val activity = ActivityLifecycleMonitorRegistry.getInstance().getActivitiesInStage(Stage.RESUMED).firstOrNull()
            activity?.let { find(it.window.decorView) } != null
        }
    }
    return onMain {
        val activity = ActivityLifecycleMonitorRegistry.getInstance().getActivitiesInStage(Stage.RESUMED).first()
        body(find(activity.window.decorView)!!)
    }
}

private fun SelectionRule.reviewText(): String = onReviewText { it.text.toString() }

private fun SelectionTextView.select(part: String) {
    val start = text.indexOf(part)
    assertTrue("'$part' in '$text'", start >= 0)
    Selection.setSelection(text as Spannable, start, start + part.length)
}

/** A point on the middle of `part`, in the view's coordinates. */
private fun SelectionTextView.pointIn(part: String): Pair<Float, Float> {
    val start = text.indexOf(part)
    assertTrue("'$part' in '$text'", start >= 0)
    val offset = start + part.length / 2
    val line = layout.getLineForOffset(offset)
    val x = layout.getPrimaryHorizontal(offset) + totalPaddingLeft - scrollX
    val y = (layout.getLineTop(line) + layout.getLineBottom(line)) / 2f + totalPaddingTop - scrollY
    return x to y
}

/**
 * `part` selected, and the menu the system would show for it: its own
 * items first (Copy, Share, Assist, Cut, a Translate, Select all), then
 * the review text's callback, until the core's answer is in.
 */
private fun SelectionRule.selectionMenu(part: String): OpenMenu {
    // The text drawn once laid out with its offsets.
    waitUntil(15_000) { reviewText().contains(part) }
    val open = onReviewText { view ->
        view.select(part)
        val menu = PopupMenu(view.context, view).menu
        menu.add(Menu.NONE, android.R.id.copy, 1, "System copy")
        menu.add(Menu.NONE, android.R.id.shareText, 2, "Share")
        menu.add(Menu.NONE, android.R.id.textAssist, 3, "Email")
        menu.add(Menu.NONE, android.R.id.cut, 4, "Cut")
        menu.add(Menu.NONE, PROCESS_TEXT, 5, "Translate")
        menu.add(Menu.NONE, android.R.id.selectAll, 20, "Select all")
        val callback = view.customSelectionActionModeCallback
        val mode = TestMode(view, menu).also { it.callback = callback }
        assertTrue(callback.onCreateActionMode(mode, menu))
        assertTrue(callback.onPrepareActionMode(mode, menu))
        OpenMenu(menu, mode, callback)
    }
    // The copy items arrive with the core's answer.
    waitUntil(10_000) { open.titles().any { it.startsWith("Copy") || it.startsWith("Replace ") } }
    return open
}

private fun OpenMenu.titles(): List<String> {
    var titles = emptyList<String>()
    instrumentation.runOnMainSync { titles = (0 until menu.size()).map { menu.getItem(it).title.toString() } }
    return titles
}

private fun SelectionRule.tapMenu(open: OpenMenu, title: String) {
    onMain {
        val item = (0 until open.menu.size()).map(open.menu::getItem).single { it.title.toString() == title }
        assertTrue(open.callback.onActionItemClicked(open.mode, item))
    }
    waitForIdle()
}

/** A clipboard that holds something else first, so a stale clip can't pass for the copy. */
private fun stale() {
    awaitWindowFocus()
    instrumentation.runOnMainSync {
        clipboard().setPrimaryClip(ClipData.newPlainText("test", STALE))
    }
}

private fun clipboard(): ClipboardManager =
    instrumentation.targetContext.getSystemService(ClipboardManager::class.java)

/** The clipboard's text; this app is in front (Android shows it to that app only). */
private fun clip(): String? {
    awaitWindowFocus()
    var text: String? = null
    instrumentation.runOnMainSync { text = clipboard().primaryClip?.getItemAt(0)?.text?.toString() }
    return text
}

private fun SelectionRule.waitForClip(text: String) {
    try {
        waitUntil(10_000) { clip() == text }
    } catch (error: Throwable) {
        throw AssertionError("clipboard: '${clip()}', wanted '$text'", error)
    }
}

/** Whether the clip is marked sensitive (ClipDescription.EXTRA_IS_SENSITIVE), as a restored reply's copy is. */
private fun sensitive(): Boolean {
    awaitWindowFocus()
    var marked = false
    instrumentation.runOnMainSync {
        marked = clipboard().primaryClipDescription?.extras?.getBoolean(ClipDescription.EXTRA_IS_SENSITIVE) ?: false
    }
    return marked
}

/** A note in a new Project, its email proposed as a scan leaves it; the screen recreated to list it. */
private fun SelectionRule.note(documents: Documents): Documents.Session {
    val text = "Write to $EMAIL today."
    val project = documents.createProject("Harbour ${System.nanoTime()}").id
    val session = documents.importDocument(ImportSource.Text(text), "Note ${System.nanoTime()}", project)
    session.recognize(QuietJobs)
    val start = text.indexOf(EMAIL)
    session.seedProposals(
        listOf(Candidate(TextRange(start.toULong(), (start + EMAIL.length).toULong()), uniffi.mdoc_ffi.Category.EMAIL, 0.9f)),
    )
    recreate()
    return session
}

/** A Ready letter, its client's name replaced. */
private fun SelectionRule.ready(documents: Documents): Documents.Session {
    val text = "Dear Maya Bennett, your claim is ready."
    val session = documents.importDocument(ImportSource.Text(text), "Letter ${System.nanoTime()}")
    session.recognize(QuietJobs)
    val start = text.indexOf("Maya Bennett")
    session.addReplacement(SourceRange(start, start + 12), Category.Person)
    recreate()
    return session
}

/** The screen made again, so its lists read the new document. */
private fun SelectionRule.recreate() {
    activityRule.scenario.onActivity { it.recreate() }
    waitForIdle()
}
