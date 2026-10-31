package io.github.lawkitt.mdoc.setup

import android.net.Network
import io.github.lawkitt.mdoc.core.AppError
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

/**
 * Downloads one pinned model file on a given network (ADR 0089): the
 * connection is opened through that `Network`, so a Wi-Fi download never
 * uses cellular. An interrupted transfer resumes with a Range request; Rust
 * then verifies the file and installs it.
 */
class ModelDownloader(
    private val network: () -> Network?,
    /** True once the user cancels. */
    private val cancelled: () -> Boolean,
    /** Called while no allowed network is up: the row says "Waiting for Wi-Fi". */
    private val waiting: (Boolean) -> Unit,
) {
    fun download(url: String, to: File, onBytes: (Long) -> Unit) {
        to.parentFile?.mkdirs()
        var attempts = 0
        while (true) {
            if (cancelled()) throw AppError.Cancelled()
            val via = network()
            if (via == null) {
                waiting(true)
                Thread.sleep(2_000)
                continue
            }
            waiting(false)
            try {
                fetch(via, url, to, onBytes)
                return
            } catch (error: IOException) {
                if (cancelled()) throw AppError.Cancelled()
                // The network went away mid-transfer: wait for it, then resume.
                if (++attempts > 20) throw error
                Thread.sleep(2_000)
            }
        }
    }

    private fun fetch(via: Network, url: String, to: File, onBytes: (Long) -> Unit) {
        var target = URL(url)
        repeat(6) {
            val connection = via.openConnection(target) as HttpURLConnection
            connection.instanceFollowRedirects = false
            connection.connectTimeout = 30_000
            connection.readTimeout = 60_000
            val have = to.length()
            if (have > 0) connection.setRequestProperty("Range", "bytes=$have-")
            when (val code = connection.responseCode) {
                in 300..399 -> {
                    target = URL(target, connection.getHeaderField("Location"))
                    connection.disconnect()
                }
                200, 206 -> {
                    // 200: the server ignored the range, so start over.
                    val append = code == 206
                    var total = if (append) have else 0L
                    connection.inputStream.use { input ->
                        FileOutputStream(to, append).use { output ->
                            val buffer = ByteArray(1 shl 16)
                            while (true) {
                                if (cancelled()) throw AppError.Cancelled()
                                val read = input.read(buffer)
                                if (read < 0) break
                                output.write(buffer, 0, read)
                                total += read
                                onBytes(total)
                            }
                        }
                    }
                    return
                }
                416 -> return // Already complete.
                else -> throw AppError.Failed("The download failed (HTTP $code).")
            }
        }
        throw IOException("$url: too many redirects")
    }
}
