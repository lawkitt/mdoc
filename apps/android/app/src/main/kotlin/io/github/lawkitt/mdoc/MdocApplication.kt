package io.github.lawkitt.mdoc

import android.app.Application
import io.github.lawkitt.mdoc.core.AppCore
import io.github.lawkitt.mdoc.jobs.Jobs
import io.github.lawkitt.mdoc.setup.NetworkWatch
import io.github.lawkitt.mdoc.setup.SetupController

class MdocApplication : Application() {
    /**
     * Made on first use, off the main thread: opening the library reads the
     * Vault key from the Keystore. A failure is not kept, so Try again retries.
     */
    val core: AppCore
        get() = synchronized(this) { opened ?: openCore(this).also { opened = it } }
    private var opened: AppCore? = null

    /** How the core opens; tests swap in an isolated storage and key, or a failure. */
    @Volatile internal var openCore: (MdocApplication) -> AppCore = { AppCore.make(it) }

    /** Forgets the open core, so the next use opens it again (tests only). */
    internal fun reopenCore() = synchronized(this) {
        opened = null
        setup.refresh()
    }

    /** The camera scan in progress, which outlives Home's screen. */
    val scan: io.github.lawkitt.mdoc.core.ScanSession by lazy {
        io.github.lawkitt.mdoc.core.ScanSession(io.github.lawkitt.mdoc.core.CameraCapture.dir(this))
    }

    /** The one long job. */
    val jobs: Jobs by lazy { Jobs(this) }

    val network: NetworkWatch by lazy { NetworkWatch(this) }

    /** Model setup, shared by Home and the documents. */
    val setup: SetupController by lazy { SetupController(this, { core.models }, { core.storage }, jobs, network) }
}
