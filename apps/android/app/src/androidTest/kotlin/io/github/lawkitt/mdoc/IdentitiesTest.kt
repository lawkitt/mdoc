package io.github.lawkitt.mdoc

import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.lawkitt.mdoc.core.CaseChoice
import io.github.lawkitt.mdoc.core.Category
import io.github.lawkitt.mdoc.core.ImportSource
import io.github.lawkitt.mdoc.core.JobObserver
import io.github.lawkitt.mdoc.core.SourceRange
import org.junit.Assert.assertEquals
import org.junit.Rule
import uniffi.mdoc_ffi.Candidate
import uniffi.mdoc_ffi.TextRange
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

    @Test
    fun aLongPressKeepsAllOfAnIdentity() {
        val title = "Keep all ${System.nanoTime()}"
        val letter = "Dear Priya Shah, Priya Shah's claim is ready."
        val session = core.app.core.documents.importDocument(ImportSource.Text(letter), title, CaseChoice.New(title))
        session.recognize(object : JobObserver {
            override fun stage(stage: JobStage) = Unit

            override fun onCancel(handler: () -> Unit) = Unit
        })
        // Two proposals of one name, as Find names leaves them.
        val name = "Priya Shah".toByteArray().size
        val second = letter.toByteArray().indexOfSecond("Priya Shah".toByteArray())
        session.seedProposals(
            listOf(
                Candidate(TextRange(5uL, (5 + name).toULong()), uniffi.mdoc_ffi.Category.PERSON, 0.9f),
                Candidate(TextRange(second.toULong(), (second + name).toULong()), uniffi.mdoc_ffi.Category.PERSON, 0.9f),
            ),
        )

        compose.activityRule.scenario.onActivity { it.recreate() }
        compose.waitUntil(10_000) { compose.onAllNodes(hasText(title)).fetchSemanticsNodes().isNotEmpty() }
        compose.onNodeWithText(title).performClick()
        compose.waitUntil(10_000) { compose.onAllNodes(hasContentDescription("Identities")).fetchSemanticsNodes().isNotEmpty() }
        compose.onAllNodes(hasContentDescription("Identities")).onFirst().performClick()
        compose.waitUntil(10_000) { compose.onAllNodes(hasText("Proposed", substring = true)).fetchSemanticsNodes().isNotEmpty() }
        compose.onAllNodes(hasText("Priya Shah")).onFirst().performTouchInput { longClick() }
        compose.waitUntil(5_000) { compose.onAllNodes(hasText("Keep all")).fetchSemanticsNodes().isNotEmpty() }
        compose.onAllNodes(hasText("Keep all")).onFirst().performClick()
        // Both mentions kept: the identity leaves the list and nothing is left to review.
        compose.waitUntil(10_000) { compose.onAllNodes(hasText("PERSON_1", substring = true), useUnmergedTree = true).fetchSemanticsNodes().isEmpty() }
        assertEquals(0, session.snapshot().pendingCount)
    }
}

private fun ByteArray.indexOfSecond(needle: ByteArray): Int {
    var seen = 0
    for (i in 0..size - needle.size) {
        if (copyOfRange(i, i + needle.size).contentEquals(needle) && ++seen == 2) return i
    }
    error("fewer than two")
}
