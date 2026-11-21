package io.github.lawkitt.mdoc

import io.github.lawkitt.mdoc.core.SourceKind
import io.github.lawkitt.mdoc.home.HomeViewModel
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** A picked file shared without a name is named after its MIME type (#1022). */
class PickedFileTest {
    private fun kind(mime: String): SourceKind? =
        HomeViewModel.extensionFor(mime)?.let { HomeViewModel.kindOf(File("Title.$it")) }

    @Test
    fun aPdfAndADocxKeepTheirFormat() {
        assertEquals("pdf", HomeViewModel.extensionFor("application/pdf"))
        assertEquals(
            "docx",
            HomeViewModel.extensionFor("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        )
        assertEquals(SourceKind.Pdf, kind("application/pdf"))
    }

    @Test
    fun aJpegIsAJpeg() {
        for (mime in listOf("image/jpeg", "image/jpg", "image/pjpeg", "IMAGE/JPEG")) {
            assertEquals(mime, "jpg", HomeViewModel.extensionFor(mime))
            assertEquals(mime, SourceKind.Image, kind(mime))
        }
    }

    @Test
    fun aPngIsAPng() {
        assertEquals("png", HomeViewModel.extensionFor("image/png"))
        assertEquals(SourceKind.Image, kind("image/png"))
    }

    @Test
    fun aHeicIsAHeic() {
        assertEquals("heic", HomeViewModel.extensionFor("image/heic"))
        assertEquals(SourceKind.Image, kind("image/heic"))
    }

    @Test
    fun aHeifIsAHeif() {
        assertEquals("heif", HomeViewModel.extensionFor("image/heif"))
        assertEquals(SourceKind.Image, kind("image/heif"))
    }

    @Test
    fun aWebpIsAWebp() {
        assertEquals("webp", HomeViewModel.extensionFor("image/webp"))
        assertEquals(SourceKind.Image, kind("image/webp"))
    }

    @Test
    fun aGifIsAGif() {
        assertEquals("gif", HomeViewModel.extensionFor("image/gif"))
        assertEquals(SourceKind.Image, kind("image/gif"))
    }

    /** The core reads every page of a TIFF (#1024 item 8). */
    @Test
    fun aTiffIsATiff() {
        assertEquals("tiff", HomeViewModel.extensionFor("image/tiff"))
        assertEquals(SourceKind.Image, kind("image/tiff"))
        assertEquals(SourceKind.Image, HomeViewModel.kindOf(File("Fax.tif")))
    }

    /** Anything else is refused with the supported formats, never taken for a JPEG. */
    @Test
    fun anUnknownTypeIsRefused() {
        for (mime in listOf("image/avif", "image/bmp", "application/octet-stream", "text/plain", null)) {
            assertNull(mime, HomeViewModel.extensionFor(mime))
        }
    }
}
