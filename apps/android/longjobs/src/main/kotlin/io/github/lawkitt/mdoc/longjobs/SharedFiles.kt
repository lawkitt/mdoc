package io.github.lawkitt.mdoc.longjobs

import android.content.ContentProvider
import android.content.ContentValues
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import java.io.File

/** The test's files in its cache folder, read-only by URI grant, as a file app shares them. */
class SharedFiles : ContentProvider() {
    override fun onCreate() = true

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor =
        ParcelFileDescriptor.open(file(uri), ParcelFileDescriptor.MODE_READ_ONLY)

    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, args: Array<out String>?, order: String?): Cursor {
        val file = file(uri)
        return MatrixCursor(arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)).apply { addRow(arrayOf<Any>(file.name, file.length())) }
    }

    override fun getType(uri: Uri) = "application/pdf"

    override fun insert(uri: Uri, values: ContentValues?): Uri? = null

    override fun delete(uri: Uri, selection: String?, args: Array<out String>?) = 0

    override fun update(uri: Uri, values: ContentValues?, selection: String?, args: Array<out String>?) = 0

    private fun file(uri: Uri) = File(requireNotNull(context).cacheDir, requireNotNull(uri.lastPathSegment))

    companion object {
        fun uri(file: File): Uri = Uri.parse("content://io.github.lawkitt.mdoc.longjobs.files/${Uri.encode(file.name)}")
    }
}
