package io.github.lawkitt.mdoc

import io.github.lawkitt.mdoc.home.HomeViewModel
import org.junit.Assert.assertEquals
import org.junit.Test

/** A pasted text's title, as iOS suggests it (AppModel.suggestedTitle). */
class SuggestedTitleTest {
    private fun title(text: String) = HomeViewModel.suggestedTitle(text, "Pasted text")

    @Test
    fun theFirstLineWithoutHeadingMarks() {
        assertEquals("Settlement agreement", title("\n  # Settlement agreement\n\nThe parties…"))
        assertEquals("Dear Maya", title("Dear Maya\nThank you."))
    }

    @Test
    fun atMostSixtyCharacters() {
        assertEquals("a".repeat(60), title("a".repeat(80)))
    }

    @Test
    fun nothingToNameItFallsBack() {
        assertEquals("Pasted text", title("   \n\n"))
        assertEquals("Pasted text", title("## "))
    }
}
