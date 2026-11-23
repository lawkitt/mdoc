package io.github.lawkitt.mdoc.core

import android.app.ActivityManager
import android.content.Context
import android.system.Os
import io.github.lawkitt.mdoc.storage.CaseKeyStore
import io.github.lawkitt.mdoc.storage.KeystoreCaseKeyStore
import io.github.lawkitt.mdoc.storage.MdocStorage
import java.io.File
import uniffi.mdoc_ffi.ReplyVault
import uniffi.mdoc_ffi.Runtimes
import uniffi.mdoc_ffi.Setup
import uniffi.mdoc_ffi.Vault
import uniffi.mdoc_ffi.linkedRuntimes

/**
 * The core objects one process uses: the encrypted `Vault` of Projects,
 * documents and replies (ADR 0125) and the `Setup` of models, from
 * `mdoc-ffi`'s Kotlin bindings. Rust owns document, review, Project and
 * restore state; the app holds view state built from the snapshots these
 * return. Restore with mdoc has a handle of its own, which writes only
 * replies (`ReplyVault`, #1091 Decision 37).
 */
class AppCore(
    val storage: MdocStorage,
    val vault: Vault,
    val setup: Setup,
    private val device: DeviceConditions = DeviceConditions.Unknown,
    private val openReplies: () -> ReplyVault = { error("No reply handle.") },
) {
    /** ONNX Runtime and PDFium as the APK ships them. */
    val runtimes: Runtimes by lazy { linkedRuntimes() }

    val models = Models(setup, { runtimes }, java.io.File(storage.dir(MdocStorage.Area.Downloads), "installed.json"), device)

    val documents = Documents(vault, storage, models, device)

    /** Restore with mdoc's places, restore and save, through its own handle. */
    val replies: Replies by lazy { Replies(openReplies(), documents.lastShared) }

    companion object {
        init {
            // The core opens both beside itself (`linked_runtimes`); loading
            // them first keeps one copy of each in the process.
            System.loadLibrary("onnxruntime")
            System.loadLibrary("pdfium")
        }

        fun make(
            context: Context,
            storage: MdocStorage = MdocStorage.app(context).prepare(),
            keys: CaseKeyStore = KeystoreCaseKeyStore(storage.caseKeyFile),
            physicalMemory: Long = physicalMemory(context),
        ): AppCore {
            // OCR's PDF reader wants a cache folder; Android has no platform one.
            Os.setenv("PDF_INSPECTOR_MODEL_CACHE", File(context.cacheDir, "pdf-inspector").path, true)
            // The Vault is sealed with the existing key (#1091 Decision 21),
            // in a folder beside the old library's, which stays untouched.
            val key = keys.key()
            val vault = try {
                core { Vault.open(storage.vault.path, key) }
            } finally {
                key.fill(0)
            }
            return AppCore(
                storage = storage,
                vault = vault,
                setup = Setup(storage.dir(MdocStorage.Area.Models).path, physicalMemory.toULong()),
                device = DeviceConditions.of(context),
                openReplies = {
                    val again = keys.key()
                    try {
                        core { ReplyVault.open(storage.vault.path, again) }
                    } finally {
                        again.fill(0)
                    }
                },
            )
        }

        /** The phone's RAM, which Setup checks against the detector floor. */
        fun physicalMemory(context: Context): Long {
            val info = ActivityManager.MemoryInfo()
            context.getSystemService(ActivityManager::class.java).getMemoryInfo(info)
            return info.totalMem
        }
    }
}
