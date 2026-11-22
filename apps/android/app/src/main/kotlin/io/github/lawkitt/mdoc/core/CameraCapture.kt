package io.github.lawkitt.mdoc.core

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.Binder
import android.os.ParcelFileDescriptor
import android.os.Process
import android.provider.OpenableColumns
import java.io.File
import java.io.FileNotFoundException
import java.util.UUID

/**
 * The system camera's output: a raw file of its own under
 * `no_backup/capture/`, outside `originals/`, so a shot survives mdoc being
 * stopped while the camera is open. Home cleans it into the scan's stage on
 * return and deletes it at once ([ScanSession]); leftovers go at the next
 * start.
 */
class CameraCapture : ContentProvider() {
    override fun onCreate() = true

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        val context = requireNotNull(context)
        val file = file(context, uri) ?: throw FileNotFoundException()
        // Only mdoc and the chosen camera, which receives a write grant for
        // this URI, can open it.
        if (Binder.getCallingUid() != Process.myUid() &&
            context.checkCallingUriPermission(uri, Intent.FLAG_GRANT_WRITE_URI_PERMISSION) != PackageManager.PERMISSION_GRANTED
        ) {
            throw SecurityException("The camera output requires write access.")
        }
        return ParcelFileDescriptor.open(file, ParcelFileDescriptor.parseMode(mode))
    }

    override fun getType(uri: Uri) = "image/jpeg"

    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor {
        val columns = projection ?: arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
        return MatrixCursor(columns).apply {
            file(requireNotNull(context), uri)?.let { file ->
                addRow(columns.map {
                    when (it) {
                        OpenableColumns.DISPLAY_NAME -> "page.jpg"
                        OpenableColumns.SIZE -> file.length()
                        else -> null
                    }
                })
            }
        }
    }

    override fun insert(uri: Uri, values: ContentValues?): Uri? = throw UnsupportedOperationException()
    override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?): Int =
        throw UnsupportedOperationException()
    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int =
        throw UnsupportedOperationException()

    companion object {
        /** Where shots wait for Home: never backed up, never under `originals/`. */
        fun dir(context: Context) = File(context.noBackupFilesDir, "capture")

        /** A new, empty shot for the camera to write. */
        internal fun create(context: Context): Uri {
            val id = UUID.randomUUID().toString()
            val folder = dir(context).apply { mkdirs() }
            check(File(folder, "$id.jpg").createNewFile()) { "mdoc could not prepare the camera." }
            return Uri.parse("content://${context.packageName}.capture/$id")
        }

        /** The raw file behind `uri`, while it exists. */
        internal fun file(context: Context, uri: Uri): File? {
            val id = uri.lastPathSegment?.takeIf { uri.authority == "${context.packageName}.capture" } ?: return null
            // Only the names `create` gives: no path can reach outside the folder.
            runCatching { UUID.fromString(id) }.getOrNull()?.takeIf { it.toString() == id } ?: return null
            return File(dir(context), "$id.jpg").takeIf { it.isFile }
        }

        /** Deletes every shot but `keep`, the one a restored scan still waits for. */
        internal fun sweep(folder: File, keep: File?) {
            folder.listFiles()?.forEach { if (it != keep) it.deleteRecursively() }
        }
    }
}
