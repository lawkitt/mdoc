package io.github.lawkitt.mdoc

import android.content.ClipboardManager
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.isNotEnabled
import androidx.compose.ui.test.junit4.AndroidComposeTestRule
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.performClick
import androidx.lifecycle.Lifecycle
import androidx.test.ext.junit.rules.ActivityScenarioRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.Decision
import io.github.lawkitt.mdoc.core.Documents
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.SavedReadiness
import io.github.lawkitt.mdoc.core.Scope
import io.github.lawkitt.mdoc.core.SourceRange
import org.junit.After
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
 * Copy for AI and Share refusals in plain words (#1209 decisions 13 and
 * 14; Vault & Projects item 38) through the screens: each refusal names
 * its reason in its own sentence and carries the one step that resolves
 * it; Copy for AI and Share stay listed in the menu, disabled with why;
 * no core text and no "Something went wrong" reaches the screen. The twin
 * of iOS's `HandOffRefusalUITests`. Below the detector floor, as the CI
 * emulator is; [RefusalsDetectorTierTest] has the detector tier's steps.
 */
@RunWith(AndroidJUnit4::class)
class RefusalsTest {
    @get:Rule(order = 0)
    val core = TestCore()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    private val app get() = core.app

    @After
    fun noStandIn() {
        app.core.documents.readiness = null
    }

    /** Done when: names left to decide list Copy for AI and Share disabled with why; Replace 1 and copy copies the checked text. */
    @Test
    fun namesLeftToDecideOfferReplaceAndCopy() {
        val note = compose.note(app.core.documents, "Note ${System.nanoTime()}")
        compose.openDocument(note.snapshot().title)
        compose.waitForShown(hasText("Replace 1") and hasClickAction())

        compose.menuShowsRefused("1 name left to decide")
        compose.tapStep("Replace 1 and copy")
        compose.waitForText("Copied for AI")
        compose.waitForShown(hasText("Copy") and hasClickAction())
        awaitWindowFocus()
        assertEquals(app.core.vault.textForAi(note.vaultId!!), clipboard())
        assertFalse(clipboard().contains(EMAIL))
        compose.noCoreText()
    }

    /**
     * Done when: a Keep of a name the Project knows shows "1 known name is
     * written out" in Share's place, with Review, which opens the kept
     * mention; replacing it brings Share and Copy back.
     */
    @Test
    fun aKeptKnownNameOffersReviewOnTheKeptMention() {
        val note = compose.note(app.core.documents, "Note ${System.nanoTime()}")
        note.decide(note.snapshot().candidates.single().id, Decision.Keep, Scope.AllOccurrences)
        compose.openDocument(note.snapshot().title)

        compose.waitForShown(hasTestTag("hand-off-refusal"))
        compose.waitForText("1 known name is written out. A name this document's Project knows is still written out. Replace it first.")
        assertFalse("Share waits", compose.exists(hasText("Share") and hasClickAction() and onScreen and isEnabled()))
        compose.menuShowsRefused("1 known name is written out")
        compose.menuClose()

        tap("Review")
        // The kept mention's sheet: Replace resolves it.
        compose.waitForText("Keep")
        compose.shown(hasText("Replace") and hasClickAction()).performClick()
        tap("Done")
        compose.waitForShown(hasText("Share") and hasClickAction())
        compose.waitUntil(10_000) { !compose.exists(hasTestTag("hand-off-refusal")) }
        compose.noCoreText()
    }

    /**
     * Done when: a name another document of the Project added while this
     * one was open is written out here; Review opens the document again
     * from the Vault, which proposes it, on its sheet.
     */
    @Test
    fun reviewProposesANameAnotherDocumentAdded() {
        val documents = app.core.documents
        val project = documents.createProject("Harbour ${System.nanoTime()}").id
        val call = documents.importDocument(ImportSource.Text("Ann Lee called back."), "Call ${System.nanoTime()}", project)
        call.recognize(Quiet)
        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.openDocument(call.snapshot().title)
        compose.waitForShown(hasText("Share") and hasClickAction())

        // Away and back: meanwhile another document of the Project names Ann Lee.
        val scenario = compose.activityRule.scenario
        scenario.moveToState(Lifecycle.State.CREATED)
        val memo = documents.importDocument(ImportSource.Text("Ann Lee signed."), "Memo", project)
        memo.recognize(Quiet)
        memo.addReplacement(SourceRange(0, 7), Category.Person)
        scenario.moveToState(Lifecycle.State.RESUMED)

        compose.waitForText("1 known name is written out", substring = true)
        tap("Review")
        compose.waitForText("Keep")
        assertEquals("proposed now", 1, call.snapshot().pendingCount)
        compose.noCoreText()
    }

    /** Done when: a tap that finds a refusal shows it in a dialog, in its own words, and the bar then agrees. */
    @Test
    fun aRefusalFoundAtTheTapShowsItsDialog() {
        val letter = compose.ready(app.core.documents)
        compose.openDocument(letter.snapshot().title)
        compose.waitForShown(hasText("Copy") and hasClickAction())

        // Only a computer excludes a document from AI today: the stand-in.
        app.core.documents.readiness = { SavedReadiness.ExcludedFromAi }
        tap("Copy")
        compose.waitForShown(hasTestTag("refusal-dialog"))
        compose.waitForText("Excluded from AI")
        compose.waitForText("This document is set to stay out of AI apps. Only mdoc on a computer changes that.")
        assertFalse("no step", compose.exists(hasTestTag("refusal-step")))
        compose.noCoreText()
        tap("OK")
        compose.waitForShown(hasTestTag("hand-off-refusal"))
        compose.waitForText("Excluded from AI. This document is set to stay out of AI apps. Only mdoc on a computer changes that.")
        compose.menuShowsRefused("Excluded from AI")
        assertFalse("no step", compose.exists(hasTestTag("menu-hand-off-step")))
    }

    /** Done when: changed since its review says so in Share's place, with no step below the floor. */
    @Test
    fun changedSinceReviewHasNoStepBelowTheFloor() {
        val letter = compose.ready(app.core.documents)
        app.core.documents.readiness = { SavedReadiness.ChangedSinceReview }
        compose.openDocument(letter.snapshot().title)

        compose.waitForText("Changed since its review. The text changed after its review, so names are looked for again first.")
        assertFalse("no Find names below the floor", compose.exists(hasText("Find names") and onScreen))
        assertTrue("Paste still restores a reply", compose.exists(hasText("Paste") and hasClickAction() and onScreen))
        compose.menuShowsRefused("Changed since its review")
        assertFalse("no step", compose.exists(hasTestTag("menu-hand-off-step")))
        compose.noCoreText()
    }

    /** Done when: below the floor a document with nothing to replace goes: the bar and the menu agree. */
    @Test
    fun belowTheFloorTheBarAndTheMenuAgree() {
        val documents = app.core.documents
        val memo = documents.importDocument(ImportSource.Text("Nothing to hide here."), "Memo ${System.nanoTime()}")
        memo.recognize(Quiet)
        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.openDocument(memo.snapshot().title)
        compose.waitForShown(hasText("Share") and hasClickAction())
        compose.openMenu()
        for (tag in MENU_ITEMS) compose.waitForShown(hasTestTag(tag) and isEnabled())
        compose.menuClose()
        compose.noCoreText()
    }

    private fun tap(text: String) {
        compose.waitForShown(hasText(text) and hasClickAction() and isEnabled())
        compose.shown(hasText(text) and hasClickAction() and isEnabled()).performClick()
        compose.waitForIdle()
    }

    private fun clipboard(): String {
        var copied = ""
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            copied = app.getSystemService(ClipboardManager::class.java).primaryClip?.getItemAt(0)?.text?.toString().orEmpty()
        }
        return copied
    }
}

/**
 * The detector tier's steps (#1209): a document never scanned lists Copy
 * for AI and Share disabled with "Names not looked for yet", and its step
 * Find names runs the scan, which asks for the models the emulator lacks.
 */
@RunWith(AndroidJUnit4::class)
class RefusalsDetectorTierTest {
    /** 16 GB: the detector tier. */
    @get:Rule(order = 0)
    val core = TestCore(physicalMemory = 16L shl 30)

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    private val app get() = core.app

    @After
    fun noStandIn() {
        app.core.documents.readiness = null
    }

    /** Done when: not scanned waits for Find names, in the menu with its reason, and the step scans. */
    @Test
    fun notScannedOffersFindNames() {
        val memo = app.core.documents.importDocument(ImportSource.Text("Ann Lee signed."), "Memo ${System.nanoTime()}")
        memo.recognize(Quiet)
        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.openDocument(memo.snapshot().title)
        compose.waitForShown(hasText("Find names") and hasClickAction())

        compose.menuShowsRefused("Names not looked for yet")
        compose.tapStep("Find names")
        // The scan asked for the name finder, which the emulator lacks.
        compose.waitForText("Not now")
        compose.noCoreText()
    }

    /**
     * Done when: names marked by hand in a document never scanned wait for
     * Find names before the tap, not after it, at the detector tier.
     */
    @Test
    fun namesMarkedByHandStillWaitForFindNames() {
        val letter = compose.ready(app.core.documents)
        compose.openDocument(letter.snapshot().title)
        compose.waitForText("Names not looked for yet. mdoc looks for names before the text leaves.")
        compose.waitForShown(hasText("Find names") and hasClickAction())
        compose.menuShowsRefused("Names not looked for yet")
        compose.menuClose()
        compose.noCoreText()
    }

    /** Done when: changed since its review offers Find names at the detector tier, on the bar and in the menu. */
    @Test
    fun changedSinceReviewOffersFindNames() {
        val letter = compose.ready(app.core.documents)
        app.core.documents.readiness = { SavedReadiness.ChangedSinceReview }
        compose.openDocument(letter.snapshot().title)
        compose.waitForText("Changed since its review. The text changed after its review, so names are looked for again first.")
        compose.waitForShown(hasText("Find names") and hasClickAction())
        compose.menuShowsRefused("Changed since its review")
        compose.waitForShown(hasTestTag("menu-hand-off-step") and hasText("Find names") and isEnabled())
        compose.menuClose()
        compose.noCoreText()
    }
}

private object Quiet : JobObserver {
    override fun stage(stage: JobStage) = Unit

    override fun onCancel(handler: () -> Unit) = Unit
}

private const val EMAIL = "ann@example.com"

/** The screens under test: MainActivity's. */
private typealias MainRule = AndroidComposeTestRule<ActivityScenarioRule<MainActivity>, MainActivity>

/** The menu's hand-off items: always listed (#1209). */
private val MENU_ITEMS = listOf("menu-copy-for-ai", "menu-share", "menu-share-file")

/** A note in a new Project, its email proposed as a scan leaves it; the screen recreated to list it. */
private fun MainRule.note(documents: Documents, title: String): Documents.Session {
    val text = "Write to $EMAIL today."
    val project = documents.createProject("Harbour ${System.nanoTime()}").id
    val session = documents.importDocument(ImportSource.Text(text), title, project)
    session.recognize(Quiet)
    val start = text.indexOf(EMAIL)
    session.seedProposals(
        listOf(Candidate(TextRange(start.toULong(), (start + EMAIL.length).toULong()), uniffi.mdoc_ffi.Category.EMAIL, 0.9f)),
    )
    recreate()
    return session
}

/** A Ready letter, its client's name replaced. */
private fun MainRule.ready(documents: Documents): Documents.Session {
    val text = "Dear Maya Bennett, your claim is ready."
    val session = documents.importDocument(ImportSource.Text(text), "Letter ${System.nanoTime()}")
    session.recognize(Quiet)
    val start = text.indexOf("Maya Bennett")
    session.addReplacement(SourceRange(start, start + 12), Category.Person)
    recreate()
    return session
}

private fun MainRule.recreate() {
    activityRule.scenario.onActivity { it.recreate() }
    waitForShown(hasContentDescription("Show sidebar"))
}

private fun ComposeTestRule.openMenu() {
    waitForShown(hasContentDescription("More options"))
    shown(hasContentDescription("More options")).performClick()
    waitForIdle()
}

private fun ComposeTestRule.menuClose() {
    back()
}

/** The menu: Copy for AI, Share and Share as Markdown file listed, disabled, with `reason` under each. */
private fun ComposeTestRule.menuShowsRefused(reason: String) {
    openMenu()
    // Each item merges its label and its reason into one node.
    for (tag in MENU_ITEMS) waitForShown(hasTestTag(tag) and isNotEnabled() and hasText(reason))
}

/** The menu's step, tapped; the menu is open. */
private fun ComposeTestRule.tapStep(title: String) {
    val step = hasTestTag("menu-hand-off-step") and hasText(title) and isEnabled()
    waitForShown(step)
    shown(step).performClick()
    waitForIdle()
}

/** Neither the core's refusal text nor the generic error reaches the screen. */
private fun ComposeTestRule.noCoreText() {
    for (word in listOf("Something went wrong", "not ready for AI", "NamesNotReplaced", "NotReviewed", "ChangedSinceReview", "ExcludedFromAi")) {
        assertFalse("on screen: $word", exists(hasText(word, substring = true) and onScreen))
    }
}
