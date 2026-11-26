package io.github.lawkitt.mdoc

import android.app.Activity
import android.app.Instrumentation
import android.content.ClipData
import android.content.ClipboardManager
import android.content.ComponentName
import android.content.Intent
import android.content.IntentSender
import android.os.Parcelable
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.core.content.IntentCompat
import androidx.lifecycle.Lifecycle
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.Documents
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.SourceRange
import io.github.lawkitt.mdoc.restore.RestoreActivity
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.JobStage

/**
 * Ask AI and "Paste reply?" (Milestone 2 item 20) through the screens:
 * hidden until Settings › AI connector (preview) is on; Ask AI starts the
 * chooser with the Vault's checked text and the masked name only; back
 * from the app it picked, the banner restores a pasted reply. The twin of
 * iOS's `AskAIUITests`. The chooser never opens: a monitor takes its
 * intent, and the test reports the app picked as the chooser would.
 */
@RunWith(AndroidJUnit4::class)
class AskAiTest {
    @get:Rule(order = 0)
    val core = TestCore()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    private val app get() = core.app
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    private object Quiet : JobObserver {
        override fun stage(stage: JobStage) = Unit

        override fun onCancel(handler: () -> Unit) = Unit
    }

    /** Takes each chooser mdoc starts instead of opening it. */
    private class Choosers : Instrumentation.ActivityMonitor() {
        val started = java.util.concurrent.CopyOnWriteArrayList<Intent>()

        override fun onStartActivity(intent: Intent): Instrumentation.ActivityResult? {
            if (intent.action != Intent.ACTION_CHOOSER) return null
            started += intent
            return Instrumentation.ActivityResult(Activity.RESULT_CANCELED, null)
        }
    }

    private val choosers = Choosers()

    /** The install's own choice, put back afterwards. */
    private var preview = false

    @Before
    fun setUp() {
        instrumentation.uiAutomation.grantRuntimePermission(app.packageName, android.Manifest.permission.POST_NOTIFICATIONS)
        preview = app.preferences.aiConnectorPreview.value
        app.preferences.setAiConnectorPreview(false)
        instrumentation.addMonitor(choosers)
    }

    @After
    fun tearDown() {
        instrumentation.removeMonitor(choosers)
        app.preferences.setAiConnectorPreview(preview)
    }

    /** mdoc behind another app and back, as a trip to the AI app; `meanwhile` runs while it is away. */
    private fun awayAndBack(meanwhile: () -> Unit = {}) {
        val scenario = compose.activityRule.scenario
        scenario.moveToState(Lifecycle.State.CREATED)
        meanwhile()
        Thread.sleep(1_000)
        scenario.moveToState(Lifecycle.State.RESUMED)
        compose.waitForShown(hasText("Copy") and hasClickAction())
    }

    /** A letter titled by its client's name, Maya Bennett replaced: Ready. */
    private fun letter(): Documents.Session {
        val text = "Dear Maya Bennett, your claim is ready."
        val project = app.core.documents.createProject("Harbour ${System.nanoTime()}").id
        val session = app.core.documents.importDocument(ImportSource.Text(text), "Maya Bennett claim", project)
        session.recognize(Quiet)
        val start = text.indexOf("Maya Bennett")
        session.addReplacement(SourceRange(start, start + 12), Category.Person)
        // Below the floor: "I checked the names myself" (item 48).
        session.confirmNamesChecked()
        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.waitForShown(hasContentDescription("Show sidebar"))
        return session
    }

    /** The open document's bar, once its hand-off shows. */
    private fun open(session: Documents.Session) {
        compose.openDocument(session.snapshot().title)
        compose.waitForShown(hasText("Copy") and hasClickAction())
    }

    private fun tap(text: String) {
        compose.waitForShown(hasText(text) and hasClickAction())
        compose.shown(hasText(text) and hasClickAction()).performClick()
        compose.waitForIdle()
    }

    private fun shows(text: String) = compose.exists(hasText(text) and onScreen)

    /** Ask AI tapped: the chooser it started. */
    private fun askAi(): Intent {
        tap("Ask AI")
        compose.waitUntil(HOST_STALLS) { choosers.started.isNotEmpty() }
        return choosers.started.single()
    }

    /** Done when: with the preview off (the default) there is no Ask AI; Settings turns it on. */
    @Test
    fun askAiIsHiddenUntilThePreviewIsOn() {
        val letter = letter()
        open(letter)
        assertTrue("Share keeps its place", shows("Share"))
        assertFalse("no Ask AI with the preview off", shows("Ask AI"))
        compose.back()

        compose.sidebar("Settings")
        compose.waitForShown(hasTestTag("settings-ai-connector-preview"))
        compose.shown(hasTestTag("settings-ai-connector-preview")).performScrollTo().performClick()
        compose.waitUntil(HOST_STALLS) { app.preferences.aiConnectorPreview.value }
        compose.back()

        open(letter)
        compose.waitForShown(hasText("Ask AI") and hasClickAction())
        assertFalse("Share moved to the menu", shows("Share"))
    }

    /** Done when: Ask AI's chooser carries the Vault's text for AI and the masked name, nothing else. */
    @Test
    fun askAiStartsTheChooserWithTheCheckedText() {
        app.preferences.setAiConnectorPreview(true)
        val letter = letter()
        open(letter)
        val chooser = askAi()
        val id = checkNotNull(letter.vaultId)
        val title = app.core.vault.shareFileName(id).removeSuffix(".md")
        val send = checkNotNull(IntentCompat.getParcelableExtra(chooser, Intent.EXTRA_INTENT, Intent::class.java))
        assertEquals(Intent.ACTION_SEND, send.action)
        assertEquals("text/plain", send.type)
        assertEquals(setOf(Intent.EXTRA_TEXT, Intent.EXTRA_TITLE), send.extras!!.keySet())
        assertEquals(app.core.vault.textForAi(id), send.getStringExtra(Intent.EXTRA_TEXT))
        assertEquals(title, send.getStringExtra(Intent.EXTRA_TITLE))
        assertEquals(title, chooser.getCharSequenceExtra(Intent.EXTRA_TITLE)?.toString())
        for (sent in listOf(send.getStringExtra(Intent.EXTRA_TEXT), title)) {
            assertFalse("an original left: $sent", sent!!.contains("Maya Bennett"))
        }
        assertNotNull("it hears the app picked", resultSender(chooser))
        @Suppress("DEPRECATION")
        val excluded = chooser.getParcelableArrayExtra(Intent.EXTRA_EXCLUDE_COMPONENTS).orEmpty().map { it as ComponentName }
        assertEquals("Restore with mdoc is no AI app", listOf(ComponentName(app, RestoreActivity::class.java)), excluded)
    }

    /**
     * Done when: after Ask AI an app is picked and mdoc left; back in mdoc,
     * "Paste reply?" shows and its Paste restores the reply on the
     * clipboard on the Reply pane, and the banner goes.
     */
    @Test
    fun theBannerRestoresAPastedReply() {
        app.preferences.setAiConnectorPreview(true)
        val letter = letter()
        open(letter)
        val chooser = askAi()
        assertFalse("not while the chooser is up", compose.exists(hasTestTag("reply-banner")))

        // The AI app in front: mdoc stops, the chooser says which app it
        // started, and the answer is copied there, before mdoc is back.
        val sender = checkNotNull(resultSender(chooser))
        awayAndBack {
            sender.sendIntent(app, 0, Intent().putExtra(Intent.EXTRA_CHOSEN_COMPONENT, ComponentName("com.example.ai", "com.example.ai.Share") as Parcelable), null, null)
            instrumentation.runOnMainSync {
                app.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("AI", "Dear PERSON_1, the claim is filed."))
            }
        }

        compose.waitForShown(hasTestTag("reply-banner"))
        assertTrue(shows("Paste reply?", substring = true))
        // Paste is on for the answer copied while mdoc was away. Android
        // hands the clipboard only to the window with focus: a tap that
        // lands while focus is still on its way back reads nothing, and a
        // person taps again, as this does (#1233).
        val paste = hasText("Paste") and isEnabled() and hasAnyAncestor(hasTestTag("reply-banner-paste"))
        val restored = hasText("1 value restored") and onScreen
        for (attempt in 1..3) {
            awaitWindowFocus()
            compose.waitForShown(paste)
            compose.shown(paste).performClick()
            // Taken: the banner goes and the Reply pane shows; else tapped again.
            compose.waitUntil(HOST_STALLS) { compose.exists(restored) || !compose.exists(hasTestTag("reply-banner")) }
            if (compose.exists(restored) || !compose.exists(hasTestTag("reply-banner"))) break
        }
        // The Reply pane, restored (its text is a TextView's).
        compose.waitForText("1 value restored")
        compose.waitUntil(HOST_STALLS) { !compose.exists(hasTestTag("reply-banner")) }
        assertEquals("Dear Maya Bennett, the claim is filed.", letter.snapshot().reply?.restored?.text)
    }

    /**
     * Done when: no banner after Ask AI's chooser closed without a choice,
     * nor after Share from ⋯, whose chooser reports nothing to Ask AI.
     */
    @Test
    fun noBannerAfterACancelOrAPlainShare() {
        app.preferences.setAiConnectorPreview(true)
        val letter = letter()
        open(letter)
        askAi()
        awayAndBack()
        assertFalse("cancelled: nothing offered", compose.exists(hasTestTag("reply-banner")))

        compose.waitForShown(hasContentDescription("More options"))
        compose.shown(hasContentDescription("More options")).performClick()
        // The menu in place before its Share is tapped: under load its
        // opening animation still moves the items for a while (#1233).
        compose.waitForShown(hasTestTag("menu-share") and isEnabled())
        compose.waitForIdle()
        compose.shown(hasTestTag("menu-share")).performClick()
        compose.waitUntil(HOST_STALLS) { choosers.started.size == 2 }
        val share = choosers.started.last()
        assertEquals("Share's chooser reports nothing", null, resultSender(share))
        awayAndBack()
        assertFalse("a plain Share offers nothing", compose.exists(hasTestTag("reply-banner")))
    }

    /** Where the chooser reports the app picked: Android 15's key, or the one before. */
    private fun resultSender(chooser: Intent): IntentSender? =
        IntentCompat.getParcelableExtra(chooser, Intent.EXTRA_CHOOSER_RESULT_INTENT_SENDER, IntentSender::class.java)
            ?: IntentCompat.getParcelableExtra(chooser, Intent.EXTRA_CHOSEN_COMPONENT_INTENT_SENDER, IntentSender::class.java)

    private fun shows(text: String, substring: Boolean) = compose.exists(hasText(text, substring) and onScreen)
}
