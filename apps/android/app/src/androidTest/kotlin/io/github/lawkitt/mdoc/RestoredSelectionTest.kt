package io.github.lawkitt.mdoc

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.os.SystemClock
import android.text.Selection
import android.text.Spannable
import android.view.ActionMode
import android.view.KeyEvent
import android.view.Menu
import android.view.MenuInflater
import android.view.View
import android.view.ViewGroup
import android.view.textclassifier.TextClassifier
import android.widget.PopupMenu
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.performClick
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
import uniffi.mdoc_ffi.JobStage

/**
 * A restored reply's text follows item 42's selection rules (fixes
 * #1228): it holds the user's real names again, so a copy of a selection
 * is marked sensitive, as Copy reply's is, and Share, Assist, Translate
 * and Search are never offered nor acted on. Through the document's Reply
 * pane: the view's selection callback driven with a menu that holds the
 * system's items first, Ctrl+C sent as keys, and accessibility's actions.
 */
@RunWith(AndroidJUnit4::class)
class RestoredSelectionTest {
    @get:Rule(order = 0)
    val core = TestCore()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    private val app get() = core.app
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val clipboard get() = instrumentation.targetContext.getSystemService(ClipboardManager::class.java)

    private object Quiet : JobObserver {
        override fun stage(stage: JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }

    /** Done when: Copy and Ctrl+C on a restored reply copy it marked; Share, Assist, Translate and Cut are gone and do nothing. */
    @Test
    fun aRestoredReplysSelectionCopiesMarkedAndNeverShares() {
        openReply()
        val part = "Thank you, Maya Bennett"

        // The menu: the system's items first, then the view's callback.
        val ids = onMain {
            val view = reply()
            view.select(part)
            val menu = PopupMenu(view.context, view).menu
            for ((id, title) in listOf(
                android.R.id.copy to "Copy", android.R.id.shareText to "Share", android.R.id.textAssist to "Email",
                android.R.id.cut to "Cut", PROCESS_TEXT to "Translate", android.R.id.selectAll to "Select all",
            )) {
                menu.add(Menu.NONE, id, menu.size(), title)
            }
            val callback = view.customSelectionActionModeCallback
            val mode = NoMode(view, menu)
            assertTrue(callback.onCreateActionMode(mode, menu))
            assertTrue(callback.onPrepareActionMode(mode, menu))
            (0 until menu.size()).map { menu.getItem(it).itemId }
        }
        assertEquals(listOf(android.R.id.copy, android.R.id.selectAll), ids)

        // The menu's Copy and a keyboard's Ctrl+C both arrive here.
        stale()
        onMain {
            val view = reply()
            view.select(part)
            assertTrue(view.onTextContextMenuItem(android.R.id.copy))
        }
        compose.waitUntil(10_000) { clip() == part }
        assertTrue("a restored reply's copy is marked", sensitive())

        stale()
        onMain {
            val view = reply()
            view.select(part)
            for (id in listOf(android.R.id.shareText, android.R.id.textAssist, android.R.id.cut)) {
                assertTrue(view.onTextContextMenuItem(id))
            }
        }
        compose.waitForIdle()
        assertEquals("Share, Assist and Cut do nothing", STALE, clip())

        // Accessibility offers none of TextView's own actions (Share,
        // Translate, smart actions), and refuses Share's if asked.
        onMain {
            val view = reply()
            view.select(part)
            assertTrue("no text classifier reads it", view.textClassifier === TextClassifier.NO_OP)
            val info = view.createAccessibilityNodeInfo()
            val own = info.actionList.map { it.id }.filter { SelectionTextView.textViewsOwn(it) }
            assertEquals("TextView's own actions", emptyList<Int>(), own)
            assertFalse(view.performAccessibilityAction(TEXT_VIEW_SHARE, null))
        }
        compose.waitForIdle()
        assertEquals("accessibility's Share does nothing", STALE, clip())
    }

    /** Review: a keyboard's Ctrl+C, sent as keys to the focused reply, copies marked. */
    @Test
    fun ctrlCOnTheReplyCopiesMarked() {
        openReply()
        val part = "Thank you, Maya Bennett"
        stale()
        onMain {
            val view = reply()
            view.requestFocus()
            view.select(part)
        }
        val down = SystemClock.uptimeMillis()
        val meta = KeyEvent.META_CTRL_ON or KeyEvent.META_CTRL_LEFT_ON
        instrumentation.sendKeySync(KeyEvent(down, down, KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_C, 0, meta))
        instrumentation.sendKeySync(KeyEvent(down, down + 50, KeyEvent.ACTION_UP, KeyEvent.KEYCODE_C, 0, meta))
        compose.waitUntil(10_000) { clip() == part }
        assertTrue(sensitive())
    }

    /** The document's Reply pane, its restored reply shown. */
    private fun openReply() {
        val letter = shared(app.core.documents)
        letter.restoreReply("Thank you, PERSON_1. The claim is filed.")
        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.waitForIdle()
        compose.openDocument(letter.snapshot().title)
        compose.waitForShown(hasText("Reply") and hasClickAction())
        compose.shown(hasText("Reply") and hasClickAction()).performClick()
        compose.waitUntil(15_000) { onReply { it != null } }
    }

    private fun <T> onMain(body: () -> T): T {
        var result: Result<T>? = null
        instrumentation.runOnMainSync { result = runCatching(body) }
        return result!!.getOrThrow()
    }

    /** The Reply pane's text, holding the restored name; null while not shown. */
    private fun <T> onReply(body: (SelectionTextView?) -> T): T = onMain { body(findReply()) }

    private fun reply(): SelectionTextView = checkNotNull(findReply())

    private fun findReply(): SelectionTextView? {
        val activity = ActivityLifecycleMonitorRegistry.getInstance().getActivitiesInStage(Stage.RESUMED).firstOrNull() ?: return null
        return all(activity.window.decorView).firstOrNull { it.text.contains("Maya Bennett") }
    }

    private fun all(view: View): List<SelectionTextView> = when (view) {
        is SelectionTextView -> listOf(view)
        is ViewGroup -> (0 until view.childCount).flatMap { all(view.getChildAt(it)) }
        else -> emptyList()
    }

    private fun SelectionTextView.select(part: String) {
        val start = text.indexOf(part)
        assertTrue("'$part' in '$text'", start >= 0)
        Selection.setSelection(text as Spannable, start, start + part.length)
    }

    private fun stale() {
        awaitWindowFocus()
        instrumentation.runOnMainSync { clipboard.setPrimaryClip(ClipData.newPlainText("test", STALE)) }
    }

    private fun clip(): String? {
        awaitWindowFocus()
        return onMain { clipboard.primaryClip?.getItemAt(0)?.text?.toString() }
    }

    private fun sensitive(): Boolean {
        awaitWindowFocus()
        return onMain { clipboard.primaryClipDescription?.extras?.getBoolean(ClipDescription.EXTRA_IS_SENSITIVE) ?: false }
    }

    /** A letter with Maya Bennett replaced, shared for AI. */
    private fun shared(documents: Documents): Documents.Session {
        val text = "Dear Maya Bennett, your claim is ready."
        val session = documents.importDocument(ImportSource.Text(text), "Letter ${System.nanoTime()}")
        session.recognize(Quiet)
        val start = text.indexOf("Maya Bennett")
        session.addReplacement(SourceRange(start, start + 12), Category.Person)
        session.copyForAi()
        return session
    }

    /** An action mode that prepares nothing again: the menu is read once. */
    private class NoMode(private val view: View, private val menu: Menu) : ActionMode() {
        override fun invalidate() = Unit

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

    private companion object {
        const val STALE = "before"

        /** TextView's own Share action for accessibility (TextView.ACCESSIBILITY_ACTION_SHARE). */
        const val TEXT_VIEW_SHARE = 0x10000000

        /** A Translate or Search item: the system's text-processing apps. */
        val PROCESS_TEXT = View.generateViewId()
    }
}
