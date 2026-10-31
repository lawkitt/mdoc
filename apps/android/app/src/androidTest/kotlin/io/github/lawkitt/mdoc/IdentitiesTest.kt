package io.github.lawkitt.mdoc

import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.lawkitt.mdoc.core.CaseChoice
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.SourceRange
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.mdoc_ffi.JobStage

/** An identity in the Identities sheet opens its first mention, as on iOS. */
@RunWith(AndroidJUnit4::class)
class IdentitiesTest {
    @get:Rule(order = 0)
    val core = TestCore()

    @get:Rule(order = 1)
    val compose = createAndroidComposeRule<MainActivity>()

    @Test
    fun anIdentityOpensItsFirstMention() {
        val title = "Identities ${System.nanoTime()}"
        // The name only at the end of a long letter, far below the first screen.
        val letter = (1..80).joinToString("\n\n") { "Paragraph $it of the letter." } + "\n\nSigned, Priya Shah."
        val session = core.app.core.documents.importDocument(ImportSource.Text(letter), title, CaseChoice.New(title))
        session.recognize(object : JobObserver {
            override fun stage(stage: JobStage) = Unit

            override fun onCancel(handler: () -> Unit) = Unit
        })
        val start = letter.toByteArray().size - "Priya Shah.".toByteArray().size
        session.addReplacement(SourceRange(start, start + "Priya Shah".length), Category.Person)

        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.waitUntil(10_000) { compose.onAllNodes(hasText(title)).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText(title).performClick()
        compose.waitUntil(10_000) { compose.onAllNodes(hasContentDescription("Identities")).fetchSemanticsNodes().isNotEmpty() }
        compose.onAllNodes(hasContentDescription("Identities")).onFirst().performClick()
        compose.waitUntil(10_000) { compose.onAllNodes(hasText("Priya Shah")).fetchSemanticsNodes().isNotEmpty() }
        compose.onAllNodes(hasText("Priya Shah")).onFirst().performClick()
        compose.waitUntil(10_000) { compose.onAllNodes(hasText("Alias", substring = true)).fetchSemanticsNodes().isNotEmpty() }
    }
}
