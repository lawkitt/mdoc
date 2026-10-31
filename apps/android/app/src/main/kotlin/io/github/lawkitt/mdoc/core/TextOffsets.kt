package io.github.lawkitt.mdoc.core

/**
 * Converts the core's UTF-8 source offsets to the UTF-16 indices Kotlin
 * strings and Android text use, and back (ADR 0089). Built once per
 * Markdown text; the Markdown itself is never re-rendered.
 */
class TextOffsets(text: String) {
    /**
     * UTF-16 index of each UTF-8 offset, `utf8Count + 1` entries. An offset
     * inside a multi-byte code point maps to that code point's start.
     */
    private val utf16ForUtf8: IntArray

    init {
        var bytes = 0
        var i = 0
        while (i < text.length) {
            val point = text.codePointAt(i)
            bytes += utf8Width(point)
            i += Character.charCount(point)
        }
        val table = IntArray(bytes + 1)
        var at = 0
        i = 0
        while (i < text.length) {
            val point = text.codePointAt(i)
            repeat(utf8Width(point)) { table[at++] = i }
            i += Character.charCount(point)
        }
        table[at] = text.length
        utf16ForUtf8 = table
    }

    val utf8Count: Int get() = utf16ForUtf8.size - 1

    fun utf16(utf8: Long): Int = utf16ForUtf8[utf8.coerceIn(0L, utf8Count.toLong()).toInt()]

    fun utf16(utf8: ULong): Int = utf16(utf8.coerceAtMost(Long.MAX_VALUE.toULong()).toLong())

    /** The UTF-16 range of a source range, clamped to the text. */
    fun range(range: SourceRange): IntRange = utf16(range.start.toLong()) until utf16(range.end.toLong())

    /** The first UTF-8 offset at or after a UTF-16 index. */
    fun utf8(utf16: Int): Int {
        var low = 0
        var high = utf8Count
        while (low < high) {
            val mid = (low + high) / 2
            if (utf16ForUtf8[mid] < utf16) low = mid + 1 else high = mid
        }
        return low
    }

    /** A UTF-16 selection `[start, end)` as a source range. */
    fun sourceRange(start: Int, end: Int): SourceRange = SourceRange(utf8(start), utf8(end))

    private companion object {
        fun utf8Width(point: Int) = when {
            point < 0x80 -> 1
            point < 0x800 -> 2
            point < 0x10000 -> 3
            else -> 4
        }
    }
}

/** A range of the core's UTF-8 source bytes. */
data class SourceRange(val start: Int, val end: Int) {
    val length: Int get() = end - start
}
