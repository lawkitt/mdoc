package io.github.lawkitt.mdoc.ui

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color as AndroidColor
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.rememberTransformableState
import androidx.compose.foundation.gestures.transformable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.lawkitt.mdoc.R
import io.github.lawkitt.mdoc.core.OriginalSource
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * The page a document came from (ADR 0085): a PDF's pages, drawn by
 * Android's own renderer (PDFium stays in the core for reading text), or a
 * photo, both zoomable with two fingers. DOCX and pasted text have none.
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
            is OriginalSource.Image -> {
                val bitmap by produceState<Bitmap?>(null, original.file) { value = withContext(Dispatchers.IO) { decode(original.file) } }
                bitmap?.let {
                    Image(
                        it.asImageBitmap(),
                        contentDescription = stringResource(R.string.original_page, 1, 1),
                        contentScale = ContentScale.Fit,
                        modifier = Modifier.fillMaxSize().then(zoomed),
                    )
                }
            }
            is OriginalSource.Pdf -> PdfPages(original.file, Modifier.fillMaxSize().then(zoomed))
        }
    }
}

@Composable
private fun PdfPages(file: File, modifier: Modifier) {
    val renderer = remember(file) {
        runCatching { PdfRenderer(ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)) }.getOrNull()
    }
    DisposableEffect(renderer) { onDispose { renderer?.close() } }
    if (renderer == null) return
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

private fun render(renderer: PdfRenderer, index: Int, width: Int): Bitmap = renderer.openPage(index).use { page ->
    val height = (width.toFloat() * page.height / page.width).toInt().coerceAtLeast(1)
    Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888).also { bitmap ->
        bitmap.eraseColor(AndroidColor.WHITE)
        page.render(bitmap, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
    }
}

/** A photo, scaled down to at most about 2,500 px on its long side. */
private fun decode(file: File): Bitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeFile(file.path, bounds)
    var sample = 1
    while (maxOf(bounds.outWidth, bounds.outHeight) / sample > 2500) sample *= 2
    return BitmapFactory.decodeFile(file.path, BitmapFactory.Options().apply { inSampleSize = sample })
}
