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
import android.system.Os
import android.system.OsConstants
import java.io.FileDescriptor
import java.io.FileNotFoundException
import java.util.UUID

/** Gives the system camera a seekable output URI with no disk-backed file. */
class CameraCapture : ContentProvider() {
    override fun onCreate() = true

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor = synchronized(captures) {
        val capture = captures[uri] ?: throw FileNotFoundException()
        val access = ParcelFileDescriptor.parseMode(mode)
        // A dup retains the memfd's write access. Only the chosen camera,
        // which receives a write grant for this URI, can open it externally.
        if (Binder.getCallingUid() != Process.myUid() &&
            requireNotNull(context).checkCallingUriPermission(uri, Intent.FLAG_GRANT_WRITE_URI_PERMISSION) != PackageManager.PERMISSION_GRANTED
        ) {
            throw SecurityException("The camera output requires write access.")
        }
        val fd = capture.fileDescriptor
        if (access and ParcelFileDescriptor.MODE_TRUNCATE != 0) Os.ftruncate(fd, 0)
        val seek = if (access and ParcelFileDescriptor.MODE_APPEND != 0) OsConstants.SEEK_END else OsConstants.SEEK_SET
        Os.lseek(fd, 0, seek)
        capture.dup()
    }

    override fun getType(uri: Uri) = "image/jpeg"

    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor = synchronized(captures) {
        val columns = projection ?: arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
        MatrixCursor(columns).apply {
            captures[uri]?.let { capture ->
                addRow(columns.map {
                    when (it) {
                        OpenableColumns.DISPLAY_NAME -> "page.jpg"
                        OpenableColumns.SIZE -> capture.statSize
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
        private val captures = mutableMapOf<Uri, ParcelFileDescriptor>()

        internal fun create(context: Context): Uri {
            val uri = Uri.parse("content://${context.packageName}.capture/${UUID.randomUUID()}")
            val capture = duplicateAndClose(Os.memfd_create("mdoc-camera-page", OsConstants.MFD_CLOEXEC))
            synchronized(captures) { captures[uri] = capture }
            return uri
        }

        private fun duplicateAndClose(fd: FileDescriptor): ParcelFileDescriptor =
            try {
                ParcelFileDescriptor.dup(fd)
            } finally {
                Os.close(fd)
            }

        internal fun take(uri: Uri): ParcelFileDescriptor? = synchronized(captures) { captures.remove(uri) }
    }
}
