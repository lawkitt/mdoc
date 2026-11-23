package io.github.lawkitt.mdoc

import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.AppCore
import io.github.lawkitt.mdoc.storage.FixedCaseKeyStore
import io.github.lawkitt.mdoc.storage.MdocStorage
import org.junit.rules.ExternalResource
import java.util.UUID

/**
 * Gives a test its own core: fresh storage folders, its own Vault among
 * them, and a fixed key, never the app's Vault or Keystore key. Removed
 * afterwards.
 */
class TestCore(
    /**
     * The phone's RAM as the core sees it: below the floor by default, since
     * the CI emulator's own 4 GB now meets it.
     */
    private val physicalMemory: Long = BELOW_FLOOR,
) : ExternalResource() {
    val app get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as MdocApplication
    lateinit var storage: MdocStorage
        private set

    override fun before() {
        storage = MdocStorage.app(app, "test-${UUID.randomUUID()}").prepare()
        val isolated = storage
        val memory = physicalMemory
        app.openCore = { AppCore.make(it, storage = isolated, keys = FixedCaseKeyStore(), physicalMemory = memory) }
        app.reopenCore()
    }

    override fun after() {
        app.openCore = { AppCore.make(it) }
        app.reopenCore()
        storage.folders.forEach { it.deleteRecursively() }
    }
}

/** A 3 GB phone: under the detector floor (`DETECTOR_FLOOR_BYTES`, 3.25 GiB). */
const val BELOW_FLOOR: Long = 3L shl 30
