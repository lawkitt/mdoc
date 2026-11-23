package io.github.lawkitt.mdoc.core

import android.graphics.Bitmap
import android.graphics.ImageDecoder
import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import java.io.OutputStream
import java.nio.ByteBuffer
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.util.UUID
import uniffi.mdoc_ffi.imageWithoutMetadata

/**
 * The received files of imports not yet read, in the intake folder, a
 * folder each, and the one file the core imports for each intake. Once a
 * document's first save keeps its original in the Vault, its files go.
 */
class Originals(private val storage: MdocStorage) {
    sealed interface Input {
        data class Text(val text: String) : Input

        /** `needsOcr`: page images, which only text recognition can read (imports kept from before [Images]). */
        data class File(val file: java.io.File, val needsOcr: Boolean) : Input

        /** Photos or camera pages, one document of a page each, which only text recognition can read. */
        data class Images(val files: List<java.io.File>) : Input
    }

    data class Prepared(val input: Input, val kind: SourceKind) {
        /** The files the import reads. */
        val files: List<File> get() = when (input) {
            is Input.Text -> emptyList()
            is Input.File -> listOf(input.file)
            is Input.Images -> input.files
        }
        val file: File? get() = files.firstOrNull()
    }

    /**
     * Names a received file after its title (the core titles a document by
     * its file). Images go to the core as they are, a page each, the copies
     * already sanitized by [stageImage].
     */
    fun prepare(source: ImportSource, title: String): Prepared = when (source) {
        is ImportSource.Text -> Prepared(Input.Text(source.text), SourceKind.Text)
        is ImportSource.File -> if (source.kind == SourceKind.Image) {
            Prepared(Input.Images(listOf(source.file)), source.kind)
        } else {
            Prepared(Input.File(rename(source.file, title), needsOcr = false), source.kind)
        }
        is ImportSource.Pages -> {
            if (source.pages.isEmpty()) throw AppError.EmptySelection()
            Prepared(Input.Images(source.pages), source.kind)
        }
    }

    private fun rename(file: File, title: String): File {
        val target = File(file.parentFile, MdocStorage.safeFileName("$title.${file.extension}"))
        if (target == file) return file
        Files.move(file.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
        return target
    }

    fun newStage(): File = newStage(STAGE_PREFIX)

    /**
     * The stage of a camera scan in progress: kept at a start for a day
     * ([clearAbandonedStages]), so a scan whose process stopped while the
     * camera was open comes back to it.
     */
    fun newScanStage(): File = newStage(SCAN_PREFIX)

    private fun newStage(prefix: String): File = File(storage.dir(MdocStorage.Area.Originals), "$prefix${UUID.randomUUID()}").apply {
        check(mkdir()) { "mdoc could not stage the import." }
    }

    /** A finished scan's stage, made an ordinary one, and its pages there. */
    fun finishScanStage(pages: List<File>): Pair<File, List<File>> {
        val stage = checkNotNull(pages.first().parentFile)
        if (!stage.name.startsWith(SCAN_PREFIX)) return stage to pages
        val finished = File(stage.parentFile, STAGE_PREFIX + stage.name.removePrefix(SCAN_PREFIX))
        Files.move(stage.toPath(), finished.toPath(), StandardCopyOption.ATOMIC_MOVE)
        return finished to pages.map { File(finished, it.name) }
    }

    /**
     * Clean pages only. The picker and camera both supply the source bytes in
     * memory. The core rewrites JPEG, PNG, WebP and GIF without their
     * metadata; anything else Android draws upright as a JPEG, or the page
     * is refused (a TIFF: #1064).
     */
    fun stageImage(file: File, bytes: ByteArray): File {
        try {
            imageWithoutMetadata(bytes)?.let { clean ->
                val target = imageFile(file, checkNotNull(imageExtension(clean)))
                writeAtomically(target) {
                    it.write(clean)
                    true
                }
                return target
            }
            return uprightJpeg(file, bytes)
        } finally {
            bytes.fill(0)
        }
    }

    /** HEIC and other images Android can read are drawn upright into a fresh JPEG. */
    private fun uprightJpeg(file: File, bytes: ByteArray): File {
        val bitmap = ImageDecoder.decodeBitmap(ImageDecoder.createSource(ByteBuffer.wrap(bytes))) { decoder, info, _ ->
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            decoder.setTargetSampleSize(sampleSize(maxOf(info.size.width, info.size.height)))
        }
        try {
            val target = imageFile(file, "jpg")
            writeAtomically(target) { bitmap.compress(Bitmap.CompressFormat.JPEG, JPEG_QUALITY, it) }
            return target
        } finally {
            bitmap.recycle()
        }
    }

    private fun imageFile(file: File, extension: String): File {
        val stem = MdocStorage.safeFileName(file.nameWithoutExtension).take(119 - extension.length)
        return File(file.parentFile, "$stem.$extension")
    }

    /**
     * A stage survives a restart only when a pending import owns it, or it
     * is a scan in progress less than a day old.
     */
    fun clearAbandonedStages(pendingFiles: List<File>) {
        val owned = pendingFiles.mapNotNull(::importFolder).toSet()
        val dayAgo = System.currentTimeMillis() - SCAN_KEPT_MILLIS
        storage.dir(MdocStorage.Area.Originals).listFiles()?.forEach {
            if (it.canonicalFile in owned) return@forEach
            val abandoned = when {
                it.name.startsWith(STAGE_PREFIX) -> true
                it.name.startsWith(SCAN_PREFIX) -> it.lastModified() < dayAgo
                else -> false
            }
            if (abandoned) it.deleteRecursively()
        }
    }

    /** The folder an import owns: the one directly under `originals/` that holds `file`. */
    fun importFolder(file: File): File? {
        val originals = storage.dir(MdocStorage.Area.Originals).canonicalFile
        var folder = file.canonicalFile.parentFile
        while (folder != null && folder.parentFile != originals) folder = folder.parentFile
        return folder
    }

    companion object {
        private const val STAGE_PREFIX = "staged-"
        private const val SCAN_PREFIX = "scanning-"

        /** How long a scan in progress waits for its process to come back. */
        private const val SCAN_KEPT_MILLIS = 24L * 60 * 60 * 1000

        /** Recognizes the formats the core keeps without converting to JPEG. */
        internal fun imageExtension(bytes: ByteArray): String? {
            fun has(offset: Int, signature: String) = bytes.size >= offset + signature.length &&
                signature.indices.all { (bytes[offset + it].toInt() and 0xff) == signature[it].code }
            return when {
                has(0, "\u00ff\u00d8\u00ff") -> "jpg"
                has(0, "\u0089PNG\r\n\u001a\n") -> "png"
                has(0, "RIFF") && has(8, "WEBP") -> "webp"
                has(0, "GIF87a") || has(0, "GIF89a") -> "gif"
                else -> null
            }
        }

        /** Installs a fully written image, preserving any existing file on failure. */
        internal fun writeAtomically(target: File, write: (OutputStream) -> Boolean) {
            val temp = File.createTempFile(".image-", ".new", target.parentFile)
            try {
                check(temp.outputStream().use(write)) { "mdoc could not encode ${target.name}." }
                Files.move(temp.toPath(), target.toPath(), StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
            } finally {
                temp.delete()
            }
        }

        /** As the core's image builder stores a re-encoded photo. */
        private const val JPEG_QUALITY = 92

        /** The core scales a longer side down to this many pixels. */
        private const val MAX_LONG_SIDE = 4500

        /** The power of two that brings `longSide` to [MAX_LONG_SIDE] or less, so a huge photo fits in memory. */
        private fun sampleSize(longSide: Int): Int {
            var sample = 1
            while (longSide / sample > MAX_LONG_SIDE) sample *= 2
            return sample
        }
    }
}
