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

    @Test
    fun anUnknownSizeIsMeasuredWithinTheLimit() {
        val bytes = ByteArray(5000) { it.toByte() }
        val temp = File(System.getProperty("java.io.tmpdir"))
        val before = temp.list().orEmpty().count { it.startsWith("intake-") }
        assertArrayEquals(bytes, HomeViewModel.readImage(ByteArrayInputStream(bytes), declared = -1))
        assertEquals(before, temp.list().orEmpty().count { it.startsWith("intake-") })
        assertThrows(IllegalStateException::class.java) {
            HomeViewModel.readImage(ByteArrayInputStream(bytes), declared = -1, limit = 4999)
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
