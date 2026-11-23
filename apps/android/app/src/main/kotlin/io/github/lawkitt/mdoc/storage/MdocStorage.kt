package io.github.lawkitt.mdoc.storage

import android.content.Context
import java.io.File

/**
 * mdoc's on-device folders, all app-private and kept out of backups and
 * device transfers by the manifest's extraction rules. Documents, Projects,
 * originals and replies live in the encrypted Vault (ADR 0125), a folder
 * of their own beside `mdoc/`. An import the core has not read yet waits in
 * `vault-intake/`: its received files, OCR's finished pages and the list of
 * imports (`pending.json`), until its first save. A decrypted original
 * being viewed sits in `vault-original-view/` in the cache, wiped when its
 * document closes and at launch (#1091 Decision 24). Models, their
 * downloads and the case key sit in the no-backup folder, so they are never
 * offered to a backup at all.
 *
 * The old library's `documents/`, `cases/`, `imports/` and `originals/`
 * under `mdoc/` are never made, read or written (#1091 Decision 2: no
 * migrator).
 */
class MdocStorage(
    val root: File,
    private val noBackup: File,
    /** The Vault's folder, a sibling of `root`, never inside the old library. */
    val vault: File,
    /** Imports not yet read: received files, OCR's pages and `pending.json`. */
    val intake: File,
    /** Decrypted originals being viewed. */
    val viewing: File,
) {

    enum class Area(val folder: String) {
        /** Received files and camera pages, a folder per import, until its first save. */
        Originals("originals"),

        /** OCR's finished pages of imports not yet complete, so recognition resumes. */
        Ocr("ocr"),
        Models("models"),

        /** Model files downloaded and waiting for the core to install them. */
        Downloads("downloads"),

        /** The `.md` files Share writes; emptied after each share and at launch. */
        HandOff("hand-off"),
    }

    fun dir(area: Area): File = when (area) {
        Area.Models, Area.Downloads -> File(noBackup, area.folder)
        Area.Originals, Area.Ocr -> File(intake, area.folder)
        Area.HandOff -> File(root, area.folder)
    }

    /** The wrapped key that seals the Vault, beside the models in no-backup storage. */
    val caseKeyFile: File get() = File(noBackup, "case-key.bin")

    /** The list of imports not yet read. */
    val pendingFile: File get() = File(intake, "pending.json")

    /** Creates the folders. Safe at every launch. */
    fun prepare(): MdocStorage {
        Area.entries.forEach { dir(it).mkdirs() }
        vault.mkdirs()
        return this
    }

    /** Every folder this storage owns, for tests to remove. */
    internal val folders: List<File> get() = listOf(root, noBackup, vault, intake, viewing)

    companion object {
        /**
         * The app's storage, or a separate one for instrumented tests:
         * `folder` names the old folder, and the Vault, its intake and its
         * viewing copies take it as their prefix there (`vault`,
         * `vault-intake` and `vault-original-view` for the app).
         */
        fun app(context: Context, folder: String = "mdoc"): MdocStorage {
            val vault = if (folder == "mdoc") "vault" else "$folder-vault"
            return MdocStorage(
                root = File(context.filesDir, folder),
                noBackup = File(context.noBackupFilesDir, folder),
                vault = File(context.filesDir, vault),
                intake = File(context.filesDir, "$vault-intake"),
                viewing = File(context.cacheDir, "$vault-original-view"),
            )
        }

        /** A file name with no path separators or control characters. */
        fun safeFileName(name: String): String {
            val cleaned = name.map { if (it == '/' || it == '\\' || it.isISOControl()) '_' else it }
                .joinToString("").trim().trim('.')
            return cleaned.ifEmpty { "document" }.take(120)
        }
    }
}
