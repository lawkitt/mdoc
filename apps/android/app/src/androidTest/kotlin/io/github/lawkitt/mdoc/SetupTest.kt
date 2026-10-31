package io.github.lawkitt.mdoc

import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.CaseChoice
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.SourceKind
import io.github.lawkitt.mdoc.jobs.JobKind
import io.github.lawkitt.mdoc.jobs.JobOutcome
import java.io.File
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.JobStage
import uniffi.mdoc_ffi.ModelId

/**
 * Setup and a long job on an Android image (ADR 0088, 0089): text
 * recognition downloads on Wi-Fi as a foreground-service job started with
 * mdoc on screen, Rust verifies and installs it, and a scanned PDF then
 * reads exactly through it as another long job. The emulator is below the
 * detector floor, so only OCR is offered.
 */
@RunWith(AndroidJUnit4::class)
class SetupTest {
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val app get() = instrumentation.targetContext.applicationContext as MdocApplication

    @Test
    fun ocrDownloadsOnWiFiAndReadsAScan() = runBlocking {
        ActivityScenario.launch(MainActivity::class.java).use {
            val setup = app.setup
            if (setup.state.value.snapshot.ocr?.installed != true) {
                instrumentation.runOnMainSync { setup.download() }
                // Starts, then finishes: about 50 MB.
                withTimeout(15 * 60_000) { setup.state.first { it.working } }
                withTimeout(15 * 60_000) { setup.state.first { !it.working } }
            }
            assertEquals(null, setup.state.value.error)
            assertEquals(true, setup.state.value.snapshot.ocr?.installed)
            assertTrue(app.core.models.missingFiles(ModelId.OCR_ENGLISH_RUSSIAN).isEmpty())

            val pdf = File(app.cacheDir, "english.pdf")
            instrumentation.context.assets.open("english.pdf").use { input -> pdf.outputStream().use(input::copyTo) }
            val expected = instrumentation.context.assets.open("english.txt").bufferedReader().readText()
            val session = app.core.documents.importDocument(
                ImportSource.File(pdf, SourceKind.Pdf), "English scan", CaseChoice.New("English scan"),
            )
            val outcome = app.jobs.run(JobKind.Recognize, session.id, "Recognizing English scan") { observer -> session.recognize(observer) }
            assertTrue(outcome.toString(), outcome is JobOutcome.Finished)
            val text = (outcome as JobOutcome.Finished).value.markdown
                .lines().joinToString(" ") { it.trimStart('#').trim() }
            assertEquals(expected.split(Regex("\\s+")).filter(String::isNotEmpty), text.split(Regex("\\s+")).filter(String::isNotEmpty))
        }
    }
}
