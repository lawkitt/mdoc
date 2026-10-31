// The Android feasibility spike's throwaway harness (ADR 0089): installs the
// models through the core's `Setup` (downloaded here from their pinned URLs,
// verified and installed by Rust, as the app will), runs the core's
// `spike.run` on the bundled fixtures, adds what only the platform knows
// (device, thermal status, memory headroom) and writes spike-report.json to
// the app's external files folder, where adb and Firebase Test Lab can pull
// it. The same report shape as tools/ios-spike.
package io.github.lawkitt.mdoc.spike

import android.app.ActivityManager
import android.content.Context
import android.os.Build
import android.os.PowerManager
import android.os.SystemClock
import android.system.Os
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.CompletableFuture
import java.util.concurrent.Executors
import kotlin.concurrent.thread
import java.util.concurrent.TimeUnit
import uniffi.mdoc_ffi.ModelId
import uniffi.mdoc_ffi.Setup
import uniffi.mdoc_ffi.SpikeInput
import uniffi.mdoc_ffi.fp32Artifacts
import uniffi.mdoc_ffi.linkedRuntimes
import uniffi.mdoc_ffi.memory
import uniffi.mdoc_ffi.reportJson
import uniffi.mdoc_ffi.probe as probeWindow
import uniffi.mdoc_ffi.run as runSpike

object Harness {
    const val REPORT = "spike-report.json"

    private var running: CompletableFuture<String>? = null

    /**
     * The one run in this process: an activity recreated by a rotation joins
     * it, rather than starting a second that would share its download file.
     */
    @Synchronized
    fun shared(context: Context): CompletableFuture<String> = running ?: CompletableFuture<String>().also { run ->
        running = run
        thread(name = "spike") { runCatching { measure(context.applicationContext) }.fold(run::complete, run::completeExceptionally) }
    }

    fun measure(context: Context): String {
        // Setup is inside the report too: a download that never finishes or a
        // model file the core rejects still leaves a report to share.
        val downloadStarted = SystemClock.elapsedRealtime()
        val prepared = runCatching {
            // The core opens these by name; loading them first makes that certain.
            System.loadLibrary("onnxruntime")
            System.loadLibrary("pdfium")
            // OCR's PDF reader wants a cache folder; Android has no platform one.
            Os.setenv("PDF_INSPECTOR_MODEL_CACHE", File(context.cacheDir, "pdf-inspector").path, true)
            stage(context) to installModels(context)
        }
        val downloadSeconds = (SystemClock.elapsedRealtime() - downloadStarted) / 1000.0

        val activity = context.getSystemService(ActivityManager::class.java)
        val memory = ActivityManager.MemoryInfo().also { activity.getMemoryInfo(it) }
        val thermal = ThermalRecorder(context.getSystemService(PowerManager::class.java))
        val started = SystemClock.elapsedRealtime()
        val outcome = prepared.mapCatching { (data, setup) -> reportJson(runSpike(input(data, setup))) }
        val seconds = (SystemClock.elapsedRealtime() - started) / 1000.0
        val samples = thermal.stop()

        val report = outcome.fold(
            onSuccess = { JSONObject(it) },
            onFailure = { JSONObject().put("error", it.toString()).put("failed_in", if (prepared.isFailure) "setup" else "run") },
        )
        val worst = samples.maxOfOrNull { it.second } ?: PowerManager.THERMAL_STATUS_NONE
        report.put(
            "platform",
            JSONObject()
                .put("device", "${Build.MANUFACTURER} ${Build.MODEL} (${Build.DEVICE})")
                .put("system", "Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})")
                .put("simulator", isEmulator)
                .put("physical_memory_bytes", memory.totalMem)
                .put("available_memory_at_start_bytes", memory.availMem)
                .put("low_memory_threshold_bytes", memory.threshold)
                .put("abi", Build.SUPPORTED_ABIS.firstOrNull() ?: "unknown")
                .put("wall_seconds", seconds)
                .put("model_download_seconds", downloadSeconds)
                .put("thermal_worst", thermalName(worst))
                .put(
                    "thermal_samples",
                    JSONArray(samples.map { (time, status) ->
                        JSONObject().put("t", time).put("state", thermalName(status))
                    }),
                ),
        )
        // "No serious thermal throttling" (ADR 0085): SEVERE or worse fails
        // (ADR 0089). An emulator reports no thermal status, so only a phone
        // row counts.
        val budgets = report.optJSONArray("budgets") ?: JSONArray()
        budgets.put(
            JSONObject()
                .put("name", "thermal status below severe")
                .put("limit", "moderate")
                .put("measured", thermalName(worst))
                .put("pass", !isEmulator && worst < PowerManager.THERMAL_STATUS_SEVERE),
        )
        report.put("budgets", budgets)
        // What the scan budget might gain from settings (roadmap Android 0.1
        // item 2): the same window under other thread counts and XNNPACK.
        prepared.getOrNull()?.let { (data, setup) ->
            report.put("detector_settings", detectorSettings(input(data, setup)))
            report.put("detector_fp32", detectorFp32(context, input(data, setup)))
        }

        val text = report.toString(2)
        val out = context.getExternalFilesDir(null) ?: context.filesDir
        File(out, REPORT).writeText(text)
        text.lineSequence().forEach { Log.i("MdocSpike", it) }
        Log.i("MdocSpike", "MDOC_SPIKE_REPORT_WRITTEN ${File(out, REPORT)}")
        return text
    }

    /**
     * The one-window probe under each detector setting: ONNX Runtime's CPU
     * provider or XNNPACK (`GLINER2_DEVICE`), at 2, 4, 6 and 8 threads
     * (`MDOC_DETECT_THREADS`). The app ships with CPU at 4. A setting that
     * fails is recorded with its error; the environment is restored after.
     */
    private fun detectorSettings(input: SpikeInput): JSONArray {
        val results = JSONArray()
        for (device in listOf("cpu", "xnnpack")) {
            for (threads in listOf(2, 4, 6, 8)) {
                Os.setenv("GLINER2_DEVICE", device, true)
                Os.setenv("MDOC_DETECT_THREADS", threads.toString(), true)
                val row = JSONObject().put("device", device).put("threads", threads)
                runCatching { probeWindow(input, false) }
                    .onSuccess { row.put("median_seconds", it.medianSeconds).put("engine_load_seconds", it.engineLoadSeconds) }
                    .onFailure { row.put("error", it.toString()) }
                Log.i("MdocSpike", "detector setting $row")
                results.put(row)
            }
        }
        Os.unsetenv("GLINER2_DEVICE")
        Os.unsetenv("MDOC_DETECT_THREADS")
        return results
    }

    /**
     * The FP32 export on the CPU provider at 4 threads: downloaded here from
     * its pinned URLs (about 1.25 GB, kept for the next run), verified by the
     * core as it loads. Its time, and the process's memory high-water mark
     * after it, against FP16's.
     */
    private fun detectorFp32(context: Context, input: SpikeInput): JSONObject {
        val row = JSONObject()
        val root = File(context.filesDir, "models-fp32")
        val started = SystemClock.elapsedRealtime()
        runCatching {
            for (artifact in fp32Artifacts()) {
                val target = File(root, artifact.path)
                if (target.length() == artifact.bytes.toLong()) continue
                target.parentFile?.mkdirs()
                download(artifact.url, target)
            }
        }.onFailure { return row.put("error", "download: $it") }
        row.put("download_seconds", (SystemClock.elapsedRealtime() - started) / 1000.0)
        Os.setenv("GLINER2_DEVICE", "cpu", true)
        Os.setenv("MDOC_DETECT_THREADS", "4", true)
        runCatching { probeWindow(input.copy(modelRoot = root.path), true) }
            .onSuccess {
                row.put("median_seconds", it.medianSeconds).put("engine_load_seconds", it.engineLoadSeconds)
                row.put("peak_memory_bytes", memory().peakFootprintBytes.toLong())
            }
            .onFailure { row.put("error", it.toString()) }
        Os.unsetenv("GLINER2_DEVICE")
        Os.unsetenv("MDOC_DETECT_THREADS")
        Log.i("MdocSpike", "detector fp32 $row")
        return row
    }

    private fun input(data: File, setup: Setup): SpikeInput {
        val runtimes = linkedRuntimes()
        val detector = checkNotNull(setup.detectorPaths(runtimes)) { "The detector is not installed." }
        val ocr = checkNotNull(setup.ocrPaths(ModelId.OCR_ENGLISH_RUSSIAN, runtimes)) { "OCR is not installed." }
        return SpikeInput(
            fixtures = File(data, "fixtures").path,
            ocrRoot = ocr.root,
            modelRoot = detector.model,
            runtimes = runtimes,
        )
    }

    /**
     * Both OCR bundles and the detector, through the core's Setup: each file
     * downloaded from its pinned URL, then checked and moved into place by
     * Rust. The harness measures every RAM class, so it tells Setup the
     * phone has room for the detector.
     */
    private fun installModels(context: Context): Setup {
        val setup = Setup(File(context.filesDir, "models").path, ULong.MAX_VALUE)
        val wanted = listOf(ModelId.GLINER2_FP16, ModelId.OCR_ENGLISH, ModelId.OCR_ENGLISH_RUSSIAN)
        for (model in setup.catalog().filter { it.id in wanted && !it.installed }) {
            for (file in model.files) {
                val downloaded = File(context.cacheDir, "download.part")
                download(file.url, downloaded)
                setup.install(model.id, file.path, downloaded.path)
            }
        }
        val missing = setup.catalog().filter { it.id in wanted && !it.installed }.map { it.name }
        check(missing.isEmpty()) { "Not installed: $missing" }
        return setup
    }

    /** Downloads `url`, resuming with a Range request when the connection drops. */
    private fun download(url: String, to: File) {
        to.delete()
        var failure: Exception? = null
        repeat(6) {
            try {
                fetch(url, to)
                return
            } catch (error: java.io.IOException) {
                failure = error
                Log.w("MdocSpike", "download of $url stopped at ${to.length()} bytes: $error")
                Thread.sleep(2_000)
            }
        }
        throw failure!!
    }

    private fun fetch(url: String, to: File) {
        var target = URL(url)
        repeat(6) {
            val connection = target.openConnection() as HttpURLConnection
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
                    // 200 means the server ignored the range: start over.
                    val append = code == 206
                    connection.inputStream.use { input ->
                        java.io.FileOutputStream(to, append).use { input.copyTo(it, 1 shl 20) }
                    }
                    return
                }
                else -> error("$url: HTTP $code")
            }
        }
        error("$url: too many redirects")
    }

    /**
     * Copies the bundled `data` assets to app storage once per install: the
     * core reads files, and assets live inside the APK.
     */
    private fun stage(context: Context): File {
        val dir = File(context.filesDir, "spike-data")
        val marker = File(dir, ".installed")
        val install = context.packageManager.getPackageInfo(context.packageName, 0).lastUpdateTime
        if (marker.exists() && marker.readText() == install.toString()) return dir
        dir.deleteRecursively()
        copyAssets(context, "data", dir)
        marker.writeText(install.toString())
        return dir
    }

    private fun copyAssets(context: Context, path: String, to: File) {
        val children = context.assets.list(path).orEmpty()
        if (children.isEmpty()) {
            to.parentFile?.mkdirs()
            context.assets.open(path).use { input -> to.outputStream().use { input.copyTo(it, 1 shl 20) } }
            return
        }
        to.mkdirs()
        for (child in children) copyAssets(context, "$path/$child", File(to, child))
    }

    private val isEmulator: Boolean
        get() = Build.HARDWARE == "ranchu" || Build.FINGERPRINT.contains("generic")

    private fun thermalName(status: Int) = when (status) {
        PowerManager.THERMAL_STATUS_NONE -> "none"
        PowerManager.THERMAL_STATUS_LIGHT -> "light"
        PowerManager.THERMAL_STATUS_MODERATE -> "moderate"
        PowerManager.THERMAL_STATUS_SEVERE -> "severe"
        PowerManager.THERMAL_STATUS_CRITICAL -> "critical"
        PowerManager.THERMAL_STATUS_EMERGENCY -> "emergency"
        PowerManager.THERMAL_STATUS_SHUTDOWN -> "shutdown"
        else -> "unknown"
    }
}

/** The thermal status once a second while the spike runs; kept at changes and every 10 s. */
private class ThermalRecorder(private val power: PowerManager) {
    private val started = SystemClock.elapsedRealtime()
    private val samples = mutableListOf<Pair<Double, Int>>()
    private val timer = Executors.newSingleThreadScheduledExecutor()

    init {
        record()
        timer.scheduleAtFixedRate(::record, 1, 1, TimeUnit.SECONDS)
    }

    @Synchronized
    private fun record() {
        val time = (SystemClock.elapsedRealtime() - started) / 1000.0
        val status = power.currentThermalStatus
        if (samples.lastOrNull()?.second != status || time.toInt() % 10 == 0) {
            samples.add(time to status)
        }
    }

    fun stop(): List<Pair<Double, Int>> {
        timer.shutdownNow()
        record()
        return synchronized(this) { samples.toList() }
    }
}
