package io.github.lawkitt.mdoc

import android.app.Application
import io.github.lawkitt.mdoc.core.AppCore

class MdocApplication : Application() {
    /**
     * Made on first use, off the main thread: opening the library reads the
     * case key from the Keystore. A failure is not kept, so Try again retries.
     */
    val core: AppCore
        get() = synchronized(this) { opened ?: AppCore.make(this).also { opened = it } }
    private var opened: AppCore? = null
}
