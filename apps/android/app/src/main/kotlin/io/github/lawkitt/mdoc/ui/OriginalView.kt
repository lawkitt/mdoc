package io.github.lawkitt.mdoc.ui

import android.graphics.Bitmap
import android.graphics.Color as AndroidColor
import android.graphics.ImageDecoder
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.rememberTransformableState
import androidx.compose.foundation.gestures.transformable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import kotlinx.coroutines.sync.withPermit
import kotlinx.coroutines.sync.Semaphore
import java.util.concurrent.atomic.AtomicInteger
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.lawkitt.mdoc.R
import io.github.lawkitt.mdoc.core.OriginalData
import io.github.lawkitt.mdoc.core.OriginalSource
import android.system.Os
import android.system.OsConstants
import java.nio.ByteBuffer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The pages a document came from: a PDF's pages, drawn by
 * Android's own renderer (PDFium stays in the core for reading text), or
 * photos and camera pages, drawn upright, all zoomable with two fingers.
 * DOCX and pasted text have none. A saved document's original is
 * decrypted from the Vault off the main thread when first shown.
 */
@Composable
fun OriginalView(original: OriginalSource, modifier: Modifier = Modifier) {
    var scale by remember { mutableFloatStateOf(1f) }
    var offset by remember { mutableStateOf(Offset.Zero) }
    val zoom = rememberTransformableState { change, pan, _ ->
        scale = (scale * change).coerceIn(1f, 5f)
        offset = if (scale == 1f) Offset.Zero else offset + pan
    }
    val zoomed = Modifier
        // A double tap zooms in to twice the size, and out again, as photo viewers do.
        .pointerInput(Unit) {
            detectTapGestures(onDoubleTap = {
                scale = if (scale > 1f) 1f else 2f
                offset = Offset.Zero
            })
        }
        .transformable(zoom)
        .graphicsLayer(scaleX = scale, scaleY = scale, translationX = offset.x, translationY = offset.y)
    Box(modifier.background(MaterialTheme.colorScheme.surfaceVariant)) {
        when (original) {
            OriginalSource.None -> Text(
                stringResource(R.string.no_original_page),
                modifier = Modifier.align(Alignment.Center).padding(32.dp),
                textAlign = TextAlign.Center,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            is OriginalSource.Images -> if (original.files.size == 1) {
                ImagePage(original.files.single(), 1, 1, ContentScale.Fit, Modifier.fillMaxSize().then(zoomed))
            } else {
                ImagePages(original.files, Modifier.fillMaxSize().then(zoomed))
            }
            is OriginalSource.Pdf -> PdfPages(original.file, Modifier.fillMaxSize().then(zoomed))
        }
    }
}

@Composable
private fun PdfPages(file: OriginalData, modifier: Modifier) {
    val opening by produceState<Result<PdfRenderer>?>(null, file) {
        value = withContext(Dispatchers.IO) { runCatching { open(file) } }
    }
    val opened = opening ?: return
    val renderer = opened.getOrNull()
    DisposableEffect(renderer) { onDispose { renderer?.close() } }
    if (renderer == null) {
        // A PDF locked against opening, or encrypted in a way the renderer
        // can't read (it says the same for both): it takes no password
        // here, so no unlocked copy is ever made of it.
        if (opened.exceptionOrNull() is SecurityException) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text(
                    stringResource(R.string.original_pdf_locked),
                    modifier = Modifier.padding(32.dp),
                    textAlign = TextAlign.Center,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        return
    }
    BoxWithConstraints(modifier) {
        val width = with(LocalDensity.current) { maxWidth.roundToPx() }
        LazyColumn(Modifier.fillMaxSize(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            items(renderer.pageCount) { index ->
                val page by produceState<Bitmap?>(null, renderer, index, width) {
                    // A pane can be measured at zero width first (side by side on a tablet).
                    if (width > 0) value = withContext(Dispatchers.IO) { synchronized(renderer) { render(renderer, index, width) } }
                }
                page?.let {
                    Image(
                        it.asImageBitmap(),
                        contentDescription = stringResource(R.string.original_page, index + 1, renderer.pageCount),
                        contentScale = ContentScale.FillWidth,
                        modifier = Modifier.fillMaxWidth().background(Color.White),
                    )
                }
            }
        }
    }
}

/**
 * The PDF's renderer; its file closes with it, or at once if it can't open.
 * An original decrypted into memory is read from an anonymous memory file,
 * so no copy of it reaches the disk.
 */
private fun open(file: OriginalData): PdfRenderer {
    val descriptor = when (file) {
        is OriginalData.Local -> ParcelFileDescriptor.open(file.file, ParcelFileDescriptor.MODE_READ_ONLY)
        is OriginalData.Stored -> when (val decrypted = file.open()) {
            is OriginalData.Decrypted.Copy -> ParcelFileDescriptor.open(decrypted.file, ParcelFileDescriptor.MODE_READ_ONLY)
            is OriginalData.Decrypted.Memory -> inMemory(decrypted.bytes)
        }
    }
    return try {
        PdfRenderer(descriptor)
    } catch (error: Exception) {
        descriptor.close()
        throw error
    }
}

/** `bytes` in an anonymous memory file, read from its start. */
private fun inMemory(bytes: ByteArray): ParcelFileDescriptor {
    val fd = Os.memfd_create("original", 0)
    try {
        var at = 0
        while (at < bytes.size) at += Os.write(fd, bytes, at, bytes.size - at)
        Os.lseek(fd, 0, OsConstants.SEEK_SET)
        return ParcelFileDescriptor.dup(fd)
    } finally {
        Os.close(fd)
    }
}

/** Photos or camera pages one under another, as a PDF's pages are. */
@Composable
private fun ImagePages(files: List<OriginalData>, modifier: Modifier) {
    LazyColumn(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        items(files.size) { index ->
            // Page-sized until drawn, so the list lays out only the pages it shows.
            ImagePage(files[index], index + 1, files.size, ContentScale.FillWidth, Modifier.fillMaxWidth())
        }
    }
}

/** A page being drawn, drawn, or one that couldn't be drawn. */
private sealed interface PageImage {
    data object Loading : PageImage

    data class Shown(val bitmap: Bitmap) : PageImage

    data object Unavailable : PageImage
}

/**
 * One photo or page, drawn upright off the main thread, and a placeholder
 * the size of a page until it is (or in its place if it can't be), so a
 * list lays out only the pages it shows.
 */
@Composable
private fun ImagePage(file: OriginalData, number: Int, count: Int, scale: ContentScale, modifier: Modifier) {
    val description = stringResource(R.string.original_page, number, count)
    val page by produceState<PageImage>(PageImage.Loading, file) {
        value = decodes.withPermit { withContext(Dispatchers.IO) { decode(file) } }
            ?.let(PageImage::Shown) ?: PageImage.Unavailable
    }
    when (val shown = page) {
        is PageImage.Shown -> Image(
            shown.bitmap.asImageBitmap(),
            contentDescription = description,
            contentScale = scale,
            modifier = modifier,
        )
        else -> Box(
            modifier
                .aspectRatio(PAGE_ASPECT)
                .background(MaterialTheme.colorScheme.surface)
                .semantics { contentDescription = description }
                .then(if (shown == PageImage.Loading) Modifier.testTag(PAGE_PLACEHOLDER) else Modifier),
        )
    }
}

/** The test tag of a page still being drawn. */
internal const val PAGE_PLACEHOLDER = "original-page-placeholder"

/** A placeholder's shape: an A4 page. */
private const val PAGE_ASPECT = 210f / 297f

/** At most two photos decode at once, however many pages a list shows. */
private val decodes = Semaphore(2)

/** Decodes started, for the tests. */
internal object OriginalDecodes {
    val started = AtomicInteger()
}

private fun render(renderer: PdfRenderer, index: Int, width: Int): Bitmap = renderer.openPage(index).use { page ->
    val height = (width.toFloat() * page.height / page.width).toInt().coerceAtLeast(1)
    Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888).also { bitmap ->
        bitmap.eraseColor(AndroidColor.WHITE)
        page.render(bitmap, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
    }
}

/** A photo, upright by its EXIF orientation, scaled down to at most about 2,500 px on its long side. */
private fun decode(file: OriginalData): Bitmap? = runCatching {
    OriginalDecodes.started.incrementAndGet()
    val source = when (file) {
        is OriginalData.Local -> ImageDecoder.createSource(file.file)
        is OriginalData.Stored -> when (val decrypted = file.open()) {
            is OriginalData.Decrypted.Copy -> ImageDecoder.createSource(decrypted.file)
            is OriginalData.Decrypted.Memory -> ImageDecoder.createSource(ByteBuffer.wrap(decrypted.bytes))
        }
    }
    ImageDecoder.decodeBitmap(source) { decoder, info, _ ->
        var sample = 1
        while (maxOf(info.size.width, info.size.height) / sample > 2500) sample *= 2
        decoder.setTargetSampleSize(sample)
        decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
    }
}.getOrNull()
