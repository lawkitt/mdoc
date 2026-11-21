package io.github.lawkitt.mdoc.core

import android.graphics.Bitmap
import android.graphics.ImageDecoder
import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import org.json.JSONArray
import org.json.JSONObject
import uniffi.mdoc_ffi.imageWithoutMetadata

/**
 * Which source file each saved document came from, for the Original view
 * (`originals.json` beside the core's folders, paths relative to the
 * storage root), and the one file the core imports for each intake.
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

    private val index = File(storage.root, "originals.json")

    /**
     * Names a received file after its title (the core titles a document by
     * its file). Images go to the core as they are, a page each, the copies
     * kept without their metadata ([kept]).
     */
    fun prepare(source: ImportSource, title: String): Prepared = when (source) {
        is ImportSource.Text -> Prepared(Input.Text(source.text), SourceKind.Text)
        is ImportSource.File -> if (source.kind == SourceKind.Image) {
            Prepared(Input.Images(listOf(kept(source.file))), source.kind)
        } else {
            Prepared(Input.File(rename(source.file, title), needsOcr = false), source.kind)
        }
        is ImportSource.Pages -> {
            if (source.pages.isEmpty()) throw AppError.EmptySelection()
            Prepared(Input.Images(source.pages.map(::kept)), source.kind)
        }
    }

    private fun rename(file: File, title: String): File {
        val target = File(file.parentFile, MdocStorage.safeFileName("$title.${file.extension}"))
        if (target == file) return file
        target.delete()
        check(file.renameTo(target)) { "mdoc could not keep ${file.name}." }
        return target
    }

    /**
     * A photo or page as mdoc keeps it, which the core reads and the
     * Original shows (#1021): the core rewrites JPEG, PNG, WebP and GIF
     * without their metadata (GPS position, camera, dates), the pixels and
     * orientation as they were; HEIC and anything else Android draws
     * upright as a JPEG without metadata. A file neither reads stays as it
     * came, and the core refuses it when the import reads it.
     */
    private fun kept(file: File): File {
        imageWithoutMetadata(file.readBytes())?.let { clean ->
            replace(file, file) { it.write(clean) }
            return file
        }
        return uprightJpeg(file) ?: file
    }

    /** `file` decoded upright by Android, as a JPEG beside it (it goes); null when Android can't read it. */
    private fun uprightJpeg(file: File): File? {
        val bitmap = runCatching {
            ImageDecoder.decodeBitmap(ImageDecoder.createSource(file)) { decoder, info, _ ->
                decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
                decoder.setTargetSampleSize(sampleSize(maxOf(info.size.width, info.size.height)))
            }
        }.getOrNull() ?: return null
        try {
            val target = File(file.parentFile, MdocStorage.safeFileName("${file.nameWithoutExtension}.jpg"))
            replace(file, target) { bitmap.compress(Bitmap.CompressFormat.JPEG, JPEG_QUALITY, it) }
            if (target != file) file.delete()
            return target
        } finally {
            bitmap.recycle()
        }
    }

    /** Writes `target` whole through a temporary file beside `source`. */
    private fun replace(source: File, target: File, write: (java.io.OutputStream) -> Unit) {
        val temp = File(source.parentFile, ".${source.name}.new")
        temp.outputStream().use(write)
        target.delete()
        check(temp.renameTo(target)) { "mdoc could not keep ${source.name}." }
    }

    @Synchronized
    fun record(path: String, kind: SourceKind, files: List<File>) {
        val entries = load()
        val entry = entries.optJSONObject(relative(path)) ?: JSONObject()
        entry.remove("file")
        val list = JSONArray().apply { files.forEach { put(relative(it.path)) } }
        entries.put(relative(path), entry.put("kind", kind.name).put("files", list))
        save(entries)
    }

    /** Find names ran on the document, even if it found nothing. */
    @Synchronized
    fun markScanned(path: String) {
        val entries = load()
        val entry = entries.optJSONObject(relative(path)) ?: JSONObject().put("kind", SourceKind.Text.name)
        entries.put(relative(path), entry.put("scanned", true))
        save(entries)
    }

    @Synchronized
    fun scanned(path: String): Boolean = load().optJSONObject(relative(path))?.optBoolean("scanned") == true

    @Synchronized
    fun kind(path: String): SourceKind = runCatching {
        SourceKind.valueOf(load().getJSONObject(relative(path)).getString("kind"))
    }.getOrDefault(SourceKind.Text)

    /** The files a document came from; an entry recorded before several were kept has one. */
    @Synchronized
    fun files(path: String): List<File> = runCatching {
        val entry = load().getJSONObject(relative(path))
        val list = entry.optJSONArray("files")
        if (list == null) {
            listOf(File(storage.root, entry.getString("file")))
        } else {
            (0 until list.length()).map { File(storage.root, list.getString(it)) }
        }
    }.getOrDefault(emptyList())

    /** Forgets a deleted document and removes its intake folder. */
    @Synchronized
    fun remove(path: String) {
        val entries = load()
        files(path).firstOrNull()?.let(::importFolder)?.deleteRecursively()
        entries.remove(relative(path))
        save(entries)
    }

    /** The folder an import owns: the one directly under `originals/` that holds `file`. */
    fun importFolder(file: File): File? {
        val originals = storage.dir(MdocStorage.Area.Originals).canonicalFile
        var folder = file.canonicalFile.parentFile
        while (folder != null && folder.parentFile != originals) folder = folder.parentFile
        return folder
    }

    private fun relative(path: String): String = File(path).relativeTo(storage.root).path.replace('\\', '/')

    private fun load(): JSONObject = runCatching { JSONObject(index.readText()) }.getOrDefault(JSONObject())

    private companion object {
        /** As the core's image builder stores a re-encoded photo. */
        const val JPEG_QUALITY = 92

        /** The core scales a longer side down to this many pixels. */
        const val MAX_LONG_SIDE = 4500

        /** The power of two that brings `longSide` to [MAX_LONG_SIDE] or less, so a huge photo fits in memory. */
        fun sampleSize(longSide: Int): Int {
            var sample = 1
            while (longSide / sample > MAX_LONG_SIDE) sample *= 2
            return sample
        }
    }

    private fun save(entries: JSONObject) {
        index.parentFile?.mkdirs()
        val temp = File(index.path + ".new")
        temp.writeText(entries.toString())
        temp.renameTo(index)
    }
}
