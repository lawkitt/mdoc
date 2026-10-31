package io.github.lawkitt.mdoc

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color
import android.text.Spanned
import android.view.accessibility.AccessibilityNodeInfo
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.AppError
import io.github.lawkitt.mdoc.core.CandidateId
import io.github.lawkitt.mdoc.core.CaseChoice
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.Originals
import io.github.lawkitt.mdoc.core.SourceKind
import io.github.lawkitt.mdoc.core.SourceRange
import io.github.lawkitt.mdoc.document.shareIntent
import io.github.lawkitt.mdoc.jobs.JobKind
import io.github.lawkitt.mdoc.jobs.JobOutcome
import io.github.lawkitt.mdoc.ui.CandidateSpan
import io.github.lawkitt.mdoc.ui.NoCopyTextView
import io.github.lawkitt.mdoc.ui.ReviewColours
import io.github.lawkitt.mdoc.ui.styledReviewText
import java.io.File
import java.util.concurrent.CountDownLatch
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.JobStage

/** Regressions from the review of the review flow (lawkitt/mdoc#315). */
@RunWith(AndroidJUnit4::class)
class ReviewFixesTest {
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val context get() = instrumentation.targetContext
    private val app get() = context.applicationContext as MdocApplication
    private val quiet = object : JobObserver {
        override fun stage(stage: JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }

    @Test
    fun theReviewTextNeverCopies() = ActivityScenario.launch(MainActivity::class.java).use {
        // On screen: Android shows the clipboard only to the app in front.
        val clipboard = context.getSystemService(ClipboardManager::class.java)
        instrumentation.runOnMainSync {
            clipboard.setPrimaryClip(ClipData.newPlainText("test", "untouched"))
            val view = NoCopyTextView(context).apply {
                setTextIsSelectable(true)
                text = "Dear Maya Bennett"
            }
            android.text.Selection.setSelection(view.text as android.text.Spannable, 5, 17)
            // A keyboard's Ctrl+C and the menu both arrive here.
            view.onTextContextMenuItem(android.R.id.copy)
            view.onTextContextMenuItem(android.R.id.cut)
            assertFalse(view.performAccessibilityAction(AccessibilityNodeInfo.ACTION_COPY, null))
            val info = AccessibilityNodeInfo.obtain()
            view.onInitializeAccessibilityNodeInfo(info)
            assertFalse(info.actionList.contains(AccessibilityNodeInfo.AccessibilityAction.ACTION_COPY))
            assertEquals("untouched", clipboard.primaryClip?.getItemAt(0)?.text)
            assertEquals("Dear Maya Bennett", view.text.toString())
        }
    }

    @Test
    fun sharingNeverCarriesTheOriginalTitle() {
        val title = "Maya Bennett ${System.nanoTime()}"
        val text = "Letter to Maya Bennett."
        val session = app.core.documents.importDocument(ImportSource.Text(text), title, CaseChoice.New("Share test"))
        session.recognize(quiet)
        val start = text.indexOf("Maya Bennett")
        session.addReplacement(SourceRange(start, start + 12), Category.Person)
        val handOff = session.copyForAi()
        for (asFile in listOf(false, true)) {
            val chooser = shareIntent(handOff.text, handOff.fileName, asFile, context)
            val send = chooser.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java)!!
            val extras = listOfNotNull(
                chooser.getCharSequenceExtra(Intent.EXTRA_TITLE), send.getStringExtra(Intent.EXTRA_TITLE),
                send.getStringExtra(Intent.EXTRA_TEXT),
            ).joinToString("\n")
            assertFalse(extras, extras.contains("Maya Bennett"))
        }
    }

    @Test
    fun activatingACandidateOpensIt() {
        var opened: CandidateId? = null
        val session = app.core.documents.importDocument(ImportSource.Text("Dear Maya."), "Tap ${System.nanoTime()}", CaseChoice.New("Tap"))
        session.recognize(quiet)
        val snapshot = session.addReplacement(SourceRange(5, 9), Category.Person)
        instrumentation.runOnMainSync {
            val colours = ReviewColours(Color.BLACK, Color.GRAY, Color.GRAY, Color.BLUE, Color.LTGRAY, false)
            val text = styledReviewText(context, snapshot.markdown, snapshot.styling, snapshot.candidates, null, colours) { opened = it }
            val span = (text as Spanned).getSpans(0, text.length, CandidateSpan::class.java).single()
            // What TalkBack's activation calls.
            span.onClick(NoCopyTextView(context))
        }
        assertEquals(snapshot.candidates.single().id, opened)
    }

    @Test
    fun aSinglePngBecomesAJpeg() {
        val folder = File(app.core.storage.dir(io.github.lawkitt.mdoc.storage.MdocStorage.Area.Originals), "png-test").apply { mkdirs() }
        val png = File(folder, "photo.png")
        png.outputStream().use { Bitmap.createBitmap(40, 40, Bitmap.Config.ARGB_8888).compress(Bitmap.CompressFormat.PNG, 100, it) }
        val prepared = Originals(app.core.storage).prepare(ImportSource.File(png, SourceKind.Image), "Photo")
        assertEquals("jpg", prepared.file?.extension)
        assertTrue(prepared.file!!.length() > 0)
        folder.deleteRecursively()
    }

    @Test
    fun aJobOutlivesTheScreenThatStartedIt() = runBlocking {
        val release = CountDownLatch(1)
        val caller = CoroutineScope(Dispatchers.Main).launch {
            app.jobs.run(JobKind.Recognize, null, "Blocked") { release.await() }
        }
        withTimeout(10_000) { app.jobs.current.first { it != null } }
        // The screen goes away mid-job.
        caller.cancelAndJoin()
        release.countDown()
        withTimeout(10_000) { app.jobs.current.first { it == null } }
        val next = app.jobs.run(JobKind.Recognize, null, "Next") { 1 }
        assertTrue(next.toString(), next is JobOutcome.Finished)
    }

    @Test
    fun aDeletedImportNeverSaves() {
        val title = "Deleted ${System.nanoTime()}"
        val session = app.core.documents.importDocument(ImportSource.Text("A short note."), title, CaseChoice.New(title))
        app.core.documents.delete(session.id)
        val outcome = runCatching { session.recognize(quiet) }
        assertTrue(outcome.toString(), outcome.exceptionOrNull() is AppError.Cancelled)
        assertNull(app.core.documents.snapshot().documents.firstOrNull { it.title == title })
    }
}
