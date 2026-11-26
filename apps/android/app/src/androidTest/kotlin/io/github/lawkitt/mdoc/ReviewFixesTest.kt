package io.github.lawkitt.mdoc

import android.content.Intent
import android.graphics.Color
import android.text.Spanned
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.AppError
import io.github.lawkitt.mdoc.core.CandidateId
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.SourceRange
import io.github.lawkitt.mdoc.document.shareIntent
import io.github.lawkitt.mdoc.jobs.JobKind
import io.github.lawkitt.mdoc.jobs.JobOutcome
import io.github.lawkitt.mdoc.ui.CandidateSpan
import io.github.lawkitt.mdoc.ui.ReviewColours
import io.github.lawkitt.mdoc.ui.SelectionTextView
import io.github.lawkitt.mdoc.ui.styledReviewText
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
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.JobStage

/** Regressions from the review of the review flow (lawkitt/mdoc#315). */
@RunWith(AndroidJUnit4::class)
class ReviewFixesTest {
    /** Its own Vault, below the floor: names marked by hand make a document Ready to share. */
    @get:Rule
    val core = TestCore()

    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val context get() = instrumentation.targetContext
    private val app get() = context.applicationContext as MdocApplication
    private val quiet = object : JobObserver {
        override fun stage(stage: JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }

    @Test
    fun sharingNeverCarriesTheOriginalTitle() {
        val title = "Maya Bennett ${System.nanoTime()}"
        val text = "Letter to Maya Bennett."
        val session = app.core.documents.importDocument(ImportSource.Text(text), title)
        session.recognize(quiet)
        val start = text.indexOf("Maya Bennett")
        session.addReplacement(SourceRange(start, start + 12), Category.Person)
        // Below the floor: "I checked the names myself" (item 48).
        session.confirmNamesChecked()
        val handOff = session.copyForAi()
        for (asFile in listOf(false, true)) {
            val chooser = shareIntent(handOff.text, handOff.fileName, asFile, context)
            val send = androidx.core.content.IntentCompat.getParcelableExtra(chooser, Intent.EXTRA_INTENT, Intent::class.java)!!
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
        val session = app.core.documents.importDocument(ImportSource.Text("Dear Maya."), "Tap ${System.nanoTime()}")
        session.recognize(quiet)
        val snapshot = session.addReplacement(SourceRange(5, 9), Category.Person)
        instrumentation.runOnMainSync {
            val colours = ReviewColours(Color.BLACK, Color.GRAY, Color.GRAY, Color.BLUE, Color.LTGRAY, false)
            val text = styledReviewText(context, snapshot.markdown, snapshot.styling, snapshot.candidates, null, colours) { opened = it }
            val span = (text as Spanned).getSpans(0, text.length, CandidateSpan::class.java).single()
            // What TalkBack's activation calls.
            span.onClick(SelectionTextView(context))
        }
        assertEquals(snapshot.candidates.single().id, opened)
    }

    @Test
    fun aJobOutlivesTheScreenThatStartedIt() = runBlocking {
        val release = CountDownLatch(1)
        val caller = CoroutineScope(Dispatchers.Main).launch {
            app.jobs.run(JobKind.Recognize, null) { release.await() }
        }
        withTimeout(10_000) { app.jobs.current.first { it != null } }
        // The screen goes away mid-job.
        caller.cancelAndJoin()
        release.countDown()
        withTimeout(10_000) { app.jobs.current.first { it == null } }
        val next = app.jobs.run(JobKind.Recognize, null) { 1 }
        assertTrue(next.toString(), next is JobOutcome.Finished)
    }

    /**
     * Jobs back to back (#689): each one's service goes into the
     * foreground before anything stops it, so Android never ends the app
     * for a service stopped too soon, however fast the jobs come.
     */
    @Test
    fun jobsBackToBackNeverStopTheNextServiceTooSoon() = runBlocking {
        repeat(30) { round ->
            val outcome = app.jobs.run(JobKind.Recognize, null) { round }
            assertTrue(outcome.toString(), outcome is JobOutcome.Finished)
        }
        // A crash comes a few seconds after the stop that caused it.
        Thread.sleep(5_000)
        val last = app.jobs.run(JobKind.Recognize, null) { 1 }
        assertTrue(last.toString(), last is JobOutcome.Finished)
    }

    @Test
    fun aDeletedImportNeverSaves() {
        val title = "Deleted ${System.nanoTime()}"
        val session = app.core.documents.importDocument(ImportSource.Text("A short note."), title)
        app.core.documents.delete(session.id)
        val outcome = runCatching { session.recognize(quiet) }
        assertTrue(outcome.toString(), outcome.exceptionOrNull() is AppError.Cancelled)
        assertNull(app.core.documents.snapshot().documents.firstOrNull { it.title == title })
    }
}
