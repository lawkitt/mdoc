package io.github.lawkitt.mdoc

import io.github.lawkitt.mdoc.core.OriginalSource
import io.github.lawkitt.mdoc.home.HomeViewModel
import java.io.ByteArrayInputStream
import java.io.File
import java.io.InputStream
import java.nio.file.Files
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

/** A picked image read into memory, and the Original its pages make. */
class ImageIntakeTest {
    /** A stream that must never be read. */
    private val untouched = object : InputStream() {
        override fun read(): Int = error("read before the size was checked")
    }

    @Test
    fun aDeclaredSizeOverTheLimitIsRefusedBeforeAnyRead() {
        val error = assertThrows(IllegalStateException::class.java) {
            HomeViewModel.readImage(untouched, declared = 101, limit = 100, free = { Long.MAX_VALUE })
        }
        assertEquals("This image is too large to import.", error.message)
    }

    @Test
    fun aSizeThisPhoneCannotHoldIsRefusedBeforeAnyAllocation() {
        val error = assertThrows(IllegalStateException::class.java) {
            HomeViewModel.readImage(untouched, declared = 150L shl 20, free = { 256L shl 20 })
        }
        assertEquals("This image is too large to import on this phone.", error.message)
    }

    @Test
    fun aDeclaredSizeIsReadExactly() {
        val bytes = ByteArray(5000) { it.toByte() }
        assertArrayEquals(bytes, HomeViewModel.readImage(ByteArrayInputStream(bytes), declared = 5000))
    }

    /**
     * An image of unknown size is spilled to a file that is unlinked before
     * any byte is written (the order is checked, not the host's file
     * system: a Windows host can't unlink an open file, a phone can), and
     * no spill is left behind either way.
     */
    @Test
    fun anUnknownSizeIsMeasuredWithinTheLimit() {
        val bytes = ByteArray(5000) { it.toByte() }
        val temp = File(checkNotNull(System.getProperty("java.io.tmpdir")))
        val before = temp.list().orEmpty().count { it.startsWith("intake-") }
        val unlinkedAt = mutableListOf<Long>()
        val read = HomeViewModel.readImage(ByteArrayInputStream(bytes), declared = -1, unlink = { spill ->
            unlinkedAt += spill.length()
            spill.delete()
        })
        assertArrayEquals(bytes, read)
        assertEquals(listOf(0L), unlinkedAt)
        assertEquals(before, temp.list().orEmpty().count { it.startsWith("intake-") })
        // A host that can't unlink an open file: the spill still goes, right after the readback.
        assertArrayEquals(bytes, HomeViewModel.readImage(ByteArrayInputStream(bytes), declared = -1, unlink = { false }))
        assertEquals(before, temp.list().orEmpty().count { it.startsWith("intake-") })
        assertThrows(IllegalStateException::class.java) {
            HomeViewModel.readImage(ByteArrayInputStream(bytes), declared = -1, limit = 4999)
        }
        assertEquals(before, temp.list().orEmpty().count { it.startsWith("intake-") })
    }

    /** Spills an older version left named in the temporary folder go at start. */
    @Test
    fun leftoverSpillsAreSwept() {
        val folder = Files.createTempDirectory("spills").toFile()
        try {
            val spill = File(folder, "intake-123.image").apply { writeText("a photo") }
            val other = File(folder, "other.tmp").apply { writeText("x") }
            HomeViewModel.sweepSpills(folder)
            assertEquals(false, spill.exists())
            assertEquals(true, other.exists())
        } finally {
            folder.deleteRecursively()
        }
    }

    @Test
    fun aProviderThatDeclaredTooLittleIsRefused() {
        assertThrows(IllegalStateException::class.java) {
            HomeViewModel.readImage(ByteArrayInputStream(ByteArray(10)), declared = 5)
        }
    }

    /** A TIFF among photos keeps every page of the Original, each in its place. */
    @Test
    fun aMixedOriginalKeepsEveryPage() {
        val folder = Files.createTempDirectory("original").toFile()
        try {
            val pages = listOf("page-1.jpg", "page-2.tiff", "page-3.png").map { File(folder, it).apply { writeText("x") } }
            assertEquals(OriginalSource.Images(pages), OriginalSource.of(pages))
        } finally {
            folder.deleteRecursively()
        }
    }
}
