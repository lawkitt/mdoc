package io.github.lawkitt.mdoc

import android.app.Application
import io.github.lawkitt.mdoc.core.AppCore
import io.github.lawkitt.mdoc.jobs.Jobs
import io.github.lawkitt.mdoc.setup.NetworkWatch
import io.github.lawkitt.mdoc.setup.SetupController

class MdocApplication : Application() {
    /**
     * Made on first use, off the main thread: opening the library reads the
     * case key from the Keystore. A failure is not kept, so Try again retries.
     */
    val core: AppCore
        get() = synchronized(this) { opened ?: AppCore.make(this).also { opened = it } }
    private var opened: AppCore? = null

    /** The one long job (ADR 0006, 0089). */
    val jobs: Jobs by lazy { Jobs(this) }

    val network: NetworkWatch by lazy { NetworkWatch(this) }

    /** Model setup, shared by Home and the documents. */
    val setup: SetupController by lazy { SetupController(this, { core.models }, { core.storage }, jobs, network) }
}
