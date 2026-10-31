package io.github.lawkitt.mdoc

import android.accessibilityservice.AccessibilityService
import android.app.UiModeManager
import android.content.Intent
import android.graphics.Bitmap
import android.text.Spanned
import android.view.View
import android.view.ViewGroup
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.printToLog
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.CaseChoice
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.SourceKind
import io.github.lawkitt.mdoc.restore.RestoreActivity
import io.github.lawkitt.mdoc.storage.MdocStorage
import io.github.lawkitt.mdoc.ui.CandidateSpan
import io.github.lawkitt.mdoc.ui.NoCopyTextView
import java.io.File
import org.json.JSONArray
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.Candidate
import uniffi.mdoc_ffi.Category
import uniffi.mdoc_ffi.TextRange

/**
 * The UI survey (ADR 0087, 0089): every screen, light and dark, and the
 * review text and Settings at the largest font, saved as PNGs under the
 * app's external files (`survey/`), which `scripts/android-survey.sh` pulls.
 * The phone is told it has 16 GB, so name finding and its setup card show.
 * Runs only with the instrumentation argument `mdoc.survey=1`; not in CI.
 */
@RunWith(AndroidJUnit4::class)
class SurveyTest {
    @get:Rule(order = 0)
    val core = TestCore(physicalMemory = 16L shl 30)

    @get:Rule(order = 1)
    val compose = createEmptyComposeRule()

    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val app get() = core.app
    private val out get() = File(app.getExternalFilesDir(null), "survey").apply { mkdirs() }
    private lateinit var settlement: String
    private lateinit var scan: String

    @Before
    fun only() = assumeTrue(InstrumentationRegistry.getArguments().getString("mdoc.survey") == "1")

    @Test
    fun light() = walk(dark = false)

    @Test
    fun dark() = walk(dark = true)

    @Test
    fun largestFont() {
        seed()
        shell("settings put system font_scale 2.0")
        try {
            launch(dark = false) {
                open(settlement)
                shot("30-review-largest-font-light")
                // The sheet's buttons and the hand-off bar stack rather than wrap (ADR 0087).
                tapFirstCandidate()
                waitFor("Alias", substring = true)
                shot("32-candidate-sheet-largest-font-light")
                back()
                compose.onAllNodes(hasText("Replace ", substring = true)).onFirst().performClick()
                waitFor("Copy")
                shot("33-hand-off-largest-font-light")
                back()
                compose.onNodeWithContentDescriptionFirst("Settings").performClick()
                waitFor("About mdoc")
                shot("31-settings-largest-font-light")
            }
        } finally {
            shell("settings put system font_scale 1.0")
        }
    }

    /** Light or dark is set before mdoc starts, as a phone's setting would be. */
    private fun launch(dark: Boolean, steps: () -> Unit) {
        val modes = app.getSystemService(UiModeManager::class.java)
        modes.setApplicationNightMode(if (dark) UiModeManager.MODE_NIGHT_YES else UiModeManager.MODE_NIGHT_NO)
        try {
            ActivityScenario.launch(MainActivity::class.java).use { steps() }
        } finally {
            modes.setApplicationNightMode(UiModeManager.MODE_NIGHT_AUTO)
        }
    }

    private fun walk(dark: Boolean) {
        seed()
        launch(dark) { steps(if (dark) "dark" else "light") }
    }

    private fun steps(tone: String) {
        waitFor("Settlement agreement")
        shot("01-home-$tone")
        compose.onNodeWithContentDescriptionFirst("Add").performClick()
        shot("02-add-menu-$tone")
        click("Paste")
        waitFor("Paste text")
        shot("03-paste-sheet-$tone")
        back()

        open(settlement)
        shot("04-review-proposed-$tone")
        tapFirstCandidate()
        waitFor("Alias", substring = true)
        shot("05-candidate-sheet-$tone")
        back()
        compose.onNodeWithContentDescriptionFirst("Identities").performClick()
        waitFor("Identities")
        shot("06-identities-$tone")
        back()
        compose.onAllNodes(hasText("Replace ", substring = true)).onFirst().performClick()
        waitFor("Copy")
        shot("07-review-replaced-$tone")
        click("Paste")
        waitFor("Paste AI reply")
        compose.onAllNodesWithText("AI reply").onFirst().performTextInput(reply())
        shot("08-reply-sheet-$tone")
        click("Restore")
        waitFor("restored", substring = true)
        shot("09-reply-pane-$tone")
        back()

        open(scan)
        shot("10-scan-not-recognized-$tone")
        // A tablet shows the original beside the text, with no tab for it.
        if (app.resources.configuration.screenWidthDp < 840) click("Original")
        compose.waitUntil(10_000) { compose.onAllNodes(hasContentDescription("Original page 1 of 1")).fetchSemanticsNodes().isNotEmpty() }
        shot("11-original-$tone")
        back()

        compose.onNodeWithContentDescriptionFirst("Settings").performClick()
        waitFor("About mdoc")
        shot("12-settings-$tone")
        back()

        val share = Intent(app, RestoreActivity::class.java).setAction(Intent.ACTION_SEND).setType("text/plain")
            .putExtra(Intent.EXTRA_TEXT, reply())
        ActivityScenario.launch<RestoreActivity>(share).use {
            waitFor("restored", substring = true)
            shot("13-restore-with-mdoc-$tone")
        }
    }

    private fun seed() {
        val assets = instrumentation.context.assets
        val markdown = assets.open("Settlement agreement.md").bufferedReader().readText()
        val session = app.core.documents.importDocument(ImportSource.Text(markdown), "Settlement agreement", CaseChoice.New("Harrow & Finch"))
        session.recognize(Quiet)
        session.seedProposals(candidates(markdown))
        settlement = session.path!!
        val folder = File(app.core.storage.dir(MdocStorage.Area.Originals), "survey-scan").apply { mkdirs() }
        val pdf = File(folder, "Scanned letter.pdf")
        assets.open("english.pdf").use { input -> pdf.outputStream().use(input::copyTo) }
        scan = app.core.documents.importDocument(ImportSource.File(pdf, SourceKind.Pdf), "Scanned letter", CaseChoice.New("Scanned letter")).id.raw
    }

    private fun candidates(markdown: String): List<Candidate> {
        val identities = JSONArray(instrumentation.context.assets.open("identities.json").bufferedReader().readText())
        val haystack = markdown.toByteArray()
        val found = mutableListOf<Candidate>()
        for (i in 0 until identities.length()) {
            val item = identities.getJSONObject(i)
            val category = when (item.getString("category")) {
                "PERSON" -> Category.PERSON
                "ORG" -> Category.ORGANIZATION
                "EMAIL" -> Category.EMAIL
                "PHONE" -> Category.PHONE
                "ADDRESS" -> Category.ADDRESS
                "BANK" -> Category.BANK
                "DATE" -> Category.DATE
                else -> Category.OTHER
            }
            val originals = item.getJSONArray("originals")
            for (j in 0 until originals.length()) {
                val needle = originals.getString(j).toByteArray()
                var start = 0
                while (start + needle.size <= haystack.size) {
                    if (haystack.copyOfRange(start, start + needle.size).contentEquals(needle)) {
                        found += Candidate(TextRange(start.toULong(), (start + needle.size).toULong()), category, 0.9f)
                        start += needle.size
                    } else {
                        start++
                    }
                }
            }
        }
        return found.sortedWith(compareBy({ it.range.start }, { it.range.end }))
    }

    private fun reply(): String = instrumentation.context.assets.open("reply.md").bufferedReader().readText()

    // Driving the app

    private fun open(document: String) {
        val title = app.core.documents.session(io.github.lawkitt.mdoc.core.DocumentId(document)).snapshot().title
        waitFor(title)
        compose.onAllNodesWithText(title).onFirst().performClick()
        compose.waitForIdle()
        Thread.sleep(500)
    }

    private fun tapFirstCandidate() {
        instrumentation.runOnMainSync {
            val activity = androidx.test.runner.lifecycle.ActivityLifecycleMonitorRegistry.getInstance()
                .getActivitiesInStage(androidx.test.runner.lifecycle.Stage.RESUMED).first()
            val view = find(activity.window.decorView) ?: return@runOnMainSync
            val text = view.text as Spanned
            text.getSpans(0, text.length, CandidateSpan::class.java).minByOrNull { text.getSpanStart(it) }?.onClick(view)
        }
        compose.waitForIdle()
    }

    private fun find(view: View): NoCopyTextView? = when (view) {
        is NoCopyTextView -> view
        is ViewGroup -> (0 until view.childCount).firstNotNullOfOrNull { find(view.getChildAt(it)) }
        else -> null
    }

    private fun click(text: String) {
        waitFor(text)
        compose.onAllNodesWithText(text).onFirst().performClick()
        compose.waitForIdle()
    }

    private fun waitFor(text: String, substring: Boolean = false) {
        try {
            compose.waitUntil(15_000) {
                compose.onAllNodes(hasText(text, substring = substring)).fetchSemanticsNodes().isNotEmpty()
            }
        } catch (timeout: Throwable) {
            shot("zz-timeout-${text.take(20).replace(Regex("[^A-Za-z0-9]"), "-")}")
            runCatching { compose.onRoot(useUnmergedTree = false).printToLog("MdocSurvey") }
            throw timeout
        }
    }

    private fun back() {
        instrumentation.uiAutomation.performGlobalAction(AccessibilityService.GLOBAL_ACTION_BACK)
        compose.waitForIdle()
        Thread.sleep(300)
    }

    private fun shell(command: String) {
        // Read to the end, so the command has finished before the next step.
        android.os.ParcelFileDescriptor.AutoCloseInputStream(instrumentation.uiAutomation.executeShellCommand(command)).use { it.readBytes() }
    }

    private fun shot(name: String) {
        compose.waitForIdle()
        Thread.sleep(700)
        val bitmap = instrumentation.uiAutomation.takeScreenshot()
            ?: error("no screenshot for $name")
        val file = File(out, "$name.png")
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        // Copied out of the app's folder: the test run uninstalls mdoc, and its files with it.
        shell("mkdir -p $SURVEY")
        shell("cp ${file.path} $SURVEY/$name.png")
    }

    private fun androidx.compose.ui.test.junit4.ComposeTestRule.onNodeWithContentDescriptionFirst(description: String) =
        onAllNodes(hasContentDescription(description)).onFirst()

    private companion object {
        /** Where the screenshots wait for scripts/android-survey.sh to pull them. */
        const val SURVEY = "/data/local/tmp/mdoc-survey"
    }

    private object Quiet : io.github.lawkitt.mdoc.core.JobObserver {
        override fun stage(stage: uniffi.mdoc_ffi.JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }
}
