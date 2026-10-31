package io.github.lawkitt.mdoc.core

import uniffi.mdoc_ffi.DetectorPaths
import uniffi.mdoc_ffi.ModelFile
import uniffi.mdoc_ffi.ModelId
import uniffi.mdoc_ffi.ModelInfo
import uniffi.mdoc_ffi.OcrPaths
import uniffi.mdoc_ffi.Runtimes
import uniffi.mdoc_ffi.Setup
import java.io.File

/** Below the floor the phone cannot find names; it still reads and marks (ADR 0088, 0089). */
enum class DeviceTier { Detector, BelowFloor }

/** One model setup offers, with what it still needs. */
data class ModelOffer(val id: ModelId, val name: String, val bytes: Long, val installed: Boolean)

data class SetupSnapshot(val tier: DeviceTier, val offers: List<ModelOffer>) {
    val detector: ModelOffer? get() = offers.firstOrNull { it.id == ModelId.GLINER2_FP16 }
    val ocr: ModelOffer? get() = offers.firstOrNull { it.id == Models.OCR_MODEL }
}

/**
 * Models that fit the phone, over the core's `Setup`: the twin of the iOS
 * `RealSetup`. The platform downloads each pinned file; Rust verifies and
 * installs it.
 */
class Models(
    private val ffi: Setup,
    private val runtimes: () -> Runtimes,
    /** Files the core installed, by model, so a retry fetches only the rest. */
    private val installedRecord: File,
) {
    fun snapshot(): SetupSnapshot {
        val tier = if (ffi.offers(ModelId.GLINER2_FP16)) DeviceTier.Detector else DeviceTier.BelowFloor
        val offers = ffi.catalog()
            .filter { it.offered && (it.id == ModelId.GLINER2_FP16 || it.id == OCR_MODEL) }
            .map { info ->
                ModelOffer(
                    info.id,
                    if (info.title.isEmpty()) info.name else "${info.name} (${info.title})",
                    (if (info.installed) info.bytes else info.missingBytes).toLong(),
                    info.installed,
                )
            }
        return SetupSnapshot(tier, offers)
    }

    /** Every file of `model` when any is missing (the core reports only the bytes). */
    fun missingFiles(model: ModelId): List<ModelFile> {
        val info = ffi.catalog().first { it.id == model }
        return if (info.installed) emptyList() else info.files
    }

    /**
     * The files still to download: all but those recorded as installed by an
     * earlier, interrupted run, unless the record disagrees with the bytes the
     * core still misses (the twin of the iOS `RealSetup.remaining`).
     */
    @Synchronized
    fun remainingFiles(model: ModelId): List<ModelFile> {
        val info = ffi.catalog().first { it.id == model }
        if (info.installed) return emptyList()
        val done = recorded()[model.name].orEmpty()
        val left = info.files.filter { it.path !in done }
        return if (left.sumOf { it.bytes } == info.missingBytes) left else info.files
    }

    /** Verifies a downloaded file against its pinned size and SHA-256, then moves it in place. */
    @Synchronized
    fun install(model: ModelId, file: ModelFile, downloaded: File): ModelInfo {
        val info = core { ffi.install(model, file.path, downloaded.path) }
        val all = recorded().toMutableMap()
        if (info.installed) all.remove(model.name) else all[model.name] = all[model.name].orEmpty() + file.path
        installedRecord.parentFile?.mkdirs()
        installedRecord.writeText(org.json.JSONObject(all.mapValues { org.json.JSONArray(it.value.toList()) }).toString())
        return info
    }

    private fun recorded(): Map<String, Set<String>> = runCatching {
        val json = org.json.JSONObject(installedRecord.readText())
        json.keys().asSequence().associateWith { key ->
            val array = json.getJSONArray(key)
            (0 until array.length()).map(array::getString).toSet()
        }
    }.getOrDefault(emptyMap())

    fun detectorPaths(): DetectorPaths? = runCatching { ffi.detectorPaths(runtimes()) }.getOrNull()

    fun ocrPaths(): OcrPaths? = runCatching { ffi.ocrPaths(OCR_MODEL, runtimes()) }.getOrNull()

    companion object {
        /** The OCR bundle setup offers: English and Russian, the desktop's languages. */
        val OCR_MODEL = ModelId.OCR_ENGLISH_RUSSIAN
    }
}
