package io.github.lawkitt.mdoc

import androidx.test.platform.app.InstrumentationRegistry
import io.github.lawkitt.mdoc.core.AppCore
import io.github.lawkitt.mdoc.storage.FixedCaseKeyStore
import io.github.lawkitt.mdoc.storage.MdocStorage
import org.junit.rules.ExternalResource
import java.util.UUID

/**
 * Gives a test its own core: a fresh storage folder and a fixed case key,
 * never the app's library or Keystore key. Removed afterwards.
 */
class TestCore : ExternalResource() {
    val app get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as MdocApplication
    lateinit var storage: MdocStorage
        private set

    override fun before() {
        storage = MdocStorage.app(app, "test-${UUID.randomUUID()}").prepare()
        val isolated = storage
        app.openCore = { AppCore.make(it, storage = isolated, keys = FixedCaseKeyStore()) }
        app.reopenCore()
    }

    override fun after() {
        app.openCore = { AppCore.make(it) }
        app.reopenCore()
        storage.root.deleteRecursively()
        storage.dir(MdocStorage.Area.Models).parentFile?.deleteRecursively()
    }
}
