package io.github.lawkitt.mdoc

import android.content.ClipboardManager
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.DocumentPhase
import io.github.lawkitt.mdoc.core.Documents
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.JobStage
import uniffi.mdoc_ffi.Readiness

/**
 * A scanned document with nothing to replace is Ready and goes with Copy
 * for AI (#1209 decision 15; Vault & Projects item 39, Android's part), at
 * the detector tier, where a scan is what makes a document Ready. The scan
 * that found nothing is the core's own, fed no candidates: the CI emulator
 * has no name finder. An edit that adds no new name has no Android test:
 * the app edits no text yet (Android item 10).
 */
@RunWith(AndroidJUnit4::class)
class NothingToReplaceTest {
    /** 16 GB: the detector tier. */
    @get:Rule(order = 0)
    val core = TestCore(physicalMemory = 16L shl 30)

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    private val app get() = core.app

    private object Quiet : JobObserver {
        override fun stage(stage: JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }

    /** A read memo whose Find names found nothing, saved as the scan saves it. */
    private fun scannedWithNothing(documents: Documents): Documents.Session {
        val session = documents.importDocument(ImportSource.Text("The meeting moved to Thursday at ten."), "Memo ${System.nanoTime()}")
        session.recognize(Quiet)
        session.seedProposals(emptyList())
        return session
    }

    /** Done when: it is Ready in the Vault and on screen, and Copy copies the Vault's text for AI. */
    @Test
    fun aScannedDocumentWithNothingToReplaceCopiesForAi() {
        val memo = scannedWithNothing(app.core.documents)
        assertEquals(DocumentPhase.Ready, memo.snapshot().phase)
        assertEquals(Readiness.Ready, app.core.vault.readiness(memo.vaultId!!))
        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.waitForShown(hasContentDescription("Show sidebar"))

        compose.openDocument(memo.snapshot().title)
        compose.waitForShown(hasText("Copy") and hasClickAction())
        assertFalse("nothing to find again", compose.exists(hasText("Find names") and onScreen))
        // Only the app with window focus reads the clipboard (#545).
        awaitWindowFocus()
        compose.shown(hasText("Copy") and hasClickAction()).performClick()
        compose.waitForText("Copied for AI")
        awaitWindowFocus()
        var copied = ""
        InstrumentationRegistry.getInstrumentation().runOnMainSync {
            copied = app.getSystemService(ClipboardManager::class.java).primaryClip?.getItemAt(0)?.text?.toString().orEmpty()
        }
        assertEquals(app.core.vault.textForAi(memo.vaultId!!), copied)
        assertFalse(compose.exists(hasText("Something went wrong") and onScreen))
    }
}
