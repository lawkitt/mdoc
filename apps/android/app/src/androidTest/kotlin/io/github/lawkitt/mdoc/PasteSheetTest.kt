package io.github.lawkitt.mdoc

import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.home.AddSheet
import io.github.lawkitt.mdoc.home.PendingAdd
import io.github.lawkitt.mdoc.ui.MdocTheme
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Paste is one tap, as iOS's paste button: the clipboard's text, and a title from its first line. */
@RunWith(AndroidJUnit4::class)
class PasteSheetTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun pasteFillsTheTextAndSuggestsATitle() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        compose.setContent {
            MdocTheme { AddSheet(PendingAdd.Text("Pasted text"), emptyList(), onAdd = { _, _, _ -> }, onDismiss = {}) }
        }
        compose.waitForIdle()
        // The clipboard filled after the sheet is up: the button reads it at the tap.
        instrumentation.runOnMainSync {
            context.getSystemService(ClipboardManager::class.java)
                .setPrimaryClip(ClipData.newPlainText("test", "# Letter to Maya\nDear Maya, the claim is ready."))
        }
        compose.onNodeWithText("Paste").performClick()
        compose.waitForIdle()
        compose.onNode(hasSetTextAction() and hasText("Dear Maya", substring = true)).assertExists()
        compose.onNode(hasSetTextAction() and hasText("Title", substring = true)).assertTextContains("Letter to Maya")
    }
}
