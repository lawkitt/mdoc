package io.github.lawkitt.mdoc

import android.app.Application
import io.github.lawkitt.mdoc.core.AppCore

class MdocApplication : Application() {
    /**
     * Made on first use, off the main thread: opening the library reads the
     * case key from the Keystore.
     */
    val core: AppCore by lazy { AppCore.make(this) }
}
