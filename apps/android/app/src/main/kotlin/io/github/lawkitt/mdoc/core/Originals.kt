package io.github.lawkitt.mdoc.core

import android.graphics.BitmapFactory
import android.graphics.pdf.PdfDocument
import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import org.json.JSONObject

/**
 * Which source file each saved document came from, for the Original view
 * (`originals.json` beside the core's folders, paths relative to the
 * storage root), and the one file the core imports for each intake.
 */
class Originals(private val storage: MdocStorage) {
    sealed interface Input {
        data class Text(val text: String) : Input

        /** `needsOcr`: page images, which only text recognition can read. */
        data class File(val file: java.io.File, val needsOcr: Boolean) : Input
    }

    data class Prepared(val input: Input, val kind: SourceKind) {
        val file: File? get() = (input as? Input.File)?.file
    }

    private val index = File(storage.root, "originals.json")

    /** Names a received file after its title (the core titles a document by its file). */
    fun prepare(source: ImportSource, title: String): Prepared = when (source) {
        is ImportSource.Text -> Prepared(Input.Text(source.text), SourceKind.Text)
        is ImportSource.File ->
            Prepared(Input.File(rename(source.file, title), source.kind == SourceKind.Image), source.kind)
        is ImportSource.Pages -> {
            val first = source.pages.firstOrNull() ?: throw AppError.EmptySelection()
            val file = if (source.pages.size == 1) rename(first, title) else pagesPdf(source.pages, title, first.parentFile!!)
            Prepared(Input.File(file, needsOcr = true), source.kind)
        }
    }

    private fun rename(file: File, title: String): File {
        val target = File(file.parentFile, MdocStorage.safeFileName("$title.${file.extension}"))
        if (target == file) return file
        target.delete()
        check(file.renameTo(target)) { "mdoc could not keep ${file.name}." }
        return target
    }

    /** Several pages as one image-only PDF, which the core reads with OCR as a scan. */
    private fun pagesPdf(pages: List<File>, title: String, folder: File): File {
        val document = PdfDocument()
        try {
            pages.forEachIndexed { index, file ->
                val bitmap = BitmapFactory.decodeFile(file.path) ?: return@forEachIndexed
                val page = document.startPage(PdfDocument.PageInfo.Builder(bitmap.width, bitmap.height, index + 1).create())
                page.canvas.drawBitmap(bitmap, 0f, 0f, null)
                document.finishPage(page)
                bitmap.recycle()
            }
            if (document.pages.isEmpty()) throw AppError.Failed("The scanned pages could not be read.")
            val target = File(folder, MdocStorage.safeFileName("$title.pdf"))
            target.outputStream().use(document::writeTo)
            return target
        } finally {
            document.close()
        }
    }

    @Synchronized
    fun record(path: String, kind: SourceKind, file: File?) {
        val entries = load()
        entries.put(relative(path), JSONObject().put("kind", kind.name).put("file", file?.let { relative(it.path) }))
        save(entries)
    }

    @Synchronized
    fun kind(path: String): SourceKind = runCatching {
        SourceKind.valueOf(load().getJSONObject(relative(path)).getString("kind"))
    }.getOrDefault(SourceKind.Text)

    @Synchronized
    fun file(path: String): File? = runCatching {
        File(storage.root, load().getJSONObject(relative(path)).getString("file"))
    }.getOrNull()

    /** Forgets a deleted document and removes its intake folder. */
    @Synchronized
    fun remove(path: String) {
        val entries = load()
        file(path)?.parentFile?.takeIf { it.parentFile == storage.dir(MdocStorage.Area.Originals) }?.deleteRecursively()
        entries.remove(relative(path))
        save(entries)
    }

    private fun relative(path: String): String = File(path).relativeTo(storage.root).path.replace('\\', '/')

    private fun load(): JSONObject = runCatching { JSONObject(index.readText()) }.getOrDefault(JSONObject())

    private fun save(entries: JSONObject) {
        index.parentFile?.mkdirs()
        val temp = File(index.path + ".new")
        temp.writeText(entries.toString())
        temp.renameTo(index)
    }
}
