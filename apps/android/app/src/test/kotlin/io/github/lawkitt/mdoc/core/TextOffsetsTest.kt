package io.github.lawkitt.mdoc.core

import org.junit.Assert.assertEquals
import org.junit.Test

/** The twin of the iOS `TextOffsetsTests`: the core's UTF-8 offsets in Kotlin strings. */
class TextOffsetsTest {
    private fun utf8(text: String) = text.toByteArray(Charsets.UTF_8).size

    @Test
    fun asciiOffsetsAreEqual() {
        val offsets = TextOffsets("Maya Ellison")
        assertEquals(5 until 12, offsets.range(SourceRange(5, 12)))
    }

    @Test
    fun multiByteCodePointsShiftUtf16Offsets() {
        // "£" is 2 UTF-8 bytes and 1 UTF-16 unit; "Ж" the same; "😀" is 4 and 2.
        val text = "£4 Жанна 😀 Okafor"
        val offsets = TextOffsets(text)
        val start = utf8(text) - utf8("Okafor")
        val range = offsets.range(SourceRange(start, utf8(text)))
        assertEquals("Okafor", text.substring(range.first, range.last + 1))
        assertEquals(SourceRange(start, utf8(text)), offsets.sourceRange(range.first, range.last + 1))
    }

    @Test
    fun cyrillicRangeRoundTrips() {
        val text = "Иванов Иван"
        val offsets = TextOffsets(text)
        val range = offsets.range(SourceRange(0, utf8("Иванов")))
        assertEquals("Иванов", text.substring(range.first, range.last + 1))
        assertEquals(utf8("Иванов"), offsets.sourceRange(range.first, range.last + 1).end)
    }

    @Test
    fun combiningMarksKeepTheirBytes() {
        // "é" as "e" + U+0301: two code points, three bytes, two UTF-16 units.
        val text = "René Dubois"
        val offsets = TextOffsets(text)
        val start = utf8("René ")
        val range = offsets.range(SourceRange(start, utf8(text)))
        assertEquals("Dubois", text.substring(range.first, range.last + 1))
    }

    @Test
    fun offsetsOutsideTheTextAreClamped() {
        val offsets = TextOffsets("ab")
        assertEquals(0, offsets.utf16(-3L))
        assertEquals(2, offsets.utf16(99L))
    }
}
