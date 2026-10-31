package io.github.lawkitt.mdoc.core

import android.app.ActivityManager
import android.content.Context
import io.github.lawkitt.mdoc.storage.CaseKeyStore
import io.github.lawkitt.mdoc.storage.KeystoreCaseKeyStore
import io.github.lawkitt.mdoc.storage.MdocStorage
import uniffi.mdoc_ffi.Library
import uniffi.mdoc_ffi.Runtimes
import uniffi.mdoc_ffi.Setup
import uniffi.mdoc_ffi.linkedRuntimes

/**
 * The core objects one process uses (ADR 0085): the `Library` of cases and
 * stored documents and the `Setup` of models, from `mdoc-ffi`'s Kotlin
 * bindings. Rust owns document, review, case and restore state; the app
 * holds view state built from the snapshots these return.
 */
class AppCore(
    val storage: MdocStorage,
    val library: Library,
    val setup: Setup,
) {
    /** ONNX Runtime and PDFium as the APK ships them. */
    val runtimes: Runtimes by lazy { linkedRuntimes() }

    companion object {
        init {
            // The core opens both by name (`linked_runtimes`); loading them
            // first makes that certain whether or not they are extracted.
            System.loadLibrary("onnxruntime")
            System.loadLibrary("pdfium")
        }

        fun make(
            context: Context,
            storage: MdocStorage = MdocStorage.app(context).prepare(),
            keys: CaseKeyStore = KeystoreCaseKeyStore(storage.caseKeyFile),
            physicalMemory: Long = physicalMemory(context),
        ): AppCore = AppCore(
            storage = storage,
            library = Library(storage.root.path, keys.key()),
            setup = Setup(storage.dir(MdocStorage.Area.Models).path, physicalMemory.toULong()),
        )

        /** The phone's RAM, which Setup checks against the detector floor. */
        fun physicalMemory(context: Context): Long {
            val info = ActivityManager.MemoryInfo()
            context.getSystemService(ActivityManager::class.java).getMemoryInfo(info)
            return info.totalMem
        }
    }
}
