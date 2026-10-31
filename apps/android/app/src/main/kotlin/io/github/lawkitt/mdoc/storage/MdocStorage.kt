package io.github.lawkitt.mdoc.storage

import android.content.Context
import java.io.File
import java.io.InputStream
import java.util.UUID

/**
 * mdoc's on-device folders (ADR 0085, 0089), all app-private and kept out of
 * backups and device transfers by the manifest's extraction rules. The
 * core's `Library` keeps Markdown in `documents/`, cases in `cases/` and the
 * OCR pages of unfinished imports in `imports/`; the app keeps the source
 * files in `originals/`. Models and their downloads sit in the no-backup
 * folder too, so they are never offered to a backup at all.
 */
class MdocStorage(val root: File, private val noBackup: File) {

    enum class Area(val folder: String) {
        /** The core's Markdown documents. */
        Documents("documents"),

        /** The core's sealed cases. */
        Cases("cases"),

        /** The core's finished OCR pages of imports not yet complete. */
        Imports("imports"),

        /** Imported PDFs, DOCX files and page images, for the Original view. */
        Originals("originals"),
        Models("models"),

        /** Model files downloaded and waiting for the core to install them. */
        Downloads("downloads"),

        /** The `.md` files Share writes; emptied after each share and at launch. */
        HandOff("hand-off"),
    }

    fun dir(area: Area): File = when (area) {
        Area.Models, Area.Downloads -> File(noBackup, area.folder)
        else -> File(root, area.folder)
    }

    /** The wrapped case key, beside the models in no-backup storage. */
    val caseKeyFile: File get() = File(noBackup, "case-key.bin")

    /** Creates the folders. Safe at every launch. */
    fun prepare(): MdocStorage {
        Area.entries.forEach { dir(it).mkdirs() }
        return this
    }

    /**
     * Copies a picked file into an area, in a folder of its own so it keeps
     * its name (the core titles a document after its file).
     */
    fun importCopy(input: InputStream, name: String, area: Area = Area.Originals): File {
        val folder = File(dir(area), UUID.randomUUID().toString()).apply { mkdirs() }
        val target = File(folder, safeFileName(name))
        input.use { source -> target.outputStream().use { source.copyTo(it) } }
        return target
    }

    companion object {
        /** The app's storage, or a separate one for instrumented tests. */
        fun app(context: Context, folder: String = "mdoc"): MdocStorage = MdocStorage(
            root = File(context.filesDir, folder),
            noBackup = File(context.noBackupFilesDir, folder),
        )

        /** A file name with no path separators or control characters. */
        fun safeFileName(name: String): String {
            val cleaned = name.map { if (it == '/' || it == '\\' || it.isISOControl()) '_' else it }
                .joinToString("").trim().trim('.')
            return cleaned.ifEmpty { "document" }.take(120)
        }
    }
}
