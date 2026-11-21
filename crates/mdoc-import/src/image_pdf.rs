//! The shared image-to-PDF builder (#1024 decision 10): photos and scans
//! become a PDF in memory, one page per image, which OCR reads like a
//! scanned page and the Original shows. Nothing is written to disk.
//!
//! - A JPEG whose long side is at most [`MAX_LONG_SIDE`] passes through
//!   without re-encoding (decision 11), its metadata segments stripped.
//! - Anything else is decoded within [`limits`]: transparency goes on white,
//!   16-bit and CMYK become 8-bit, the long side is scaled to
//!   [`MAX_LONG_SIDE`] at most, and the pixels are stored with Flate for a
//!   lossless source (PNG, GIF) or as a JPEG at quality [`JPEG_QUALITY`] for
//!   a photo (JPEG, WebP).
//! - The EXIF orientation is applied through the page's content matrix,
//!   never by moving pixels, so the passthrough stays byte for byte.
//! - Metadata (EXIF, GPS, XMP, camera, dates, ICC profiles, comments) never
//!   reaches the PDF, so it can't reach the Markdown either (decision 14).
//! - An animated GIF or WebP gives its first frame; the later frames are
//!   counted for the import note.
//! - A TIFF gives every page, each sized by its own resolution; CCITT fax
//!   pages pass through as `CCITTFaxDecode` (see [`tiff`]).
//!
//! - HEIC and HEIF are decoded by the operating system ([`heic`]), then
//!   stored like any photo.
//!
//! The seam for formats is [`ImageFormat`]: each format gives [`Page`]s in
//! [`pages`].

pub mod heic;
mod jpeg;
mod pdf;
#[cfg(test)]
pub(crate) mod tests;
mod tiff;

use std::{io::Cursor, path::Path};

/// The longest side, in pixels, an image keeps: larger ones are scaled down.
/// About 15 inches at 300 dpi, more than OCR or the Original needs.
pub const MAX_LONG_SIDE: u32 = 4_500;

/// The JPEG quality a re-encoded photo is stored at.
pub const JPEG_QUALITY: u8 = 92;

/// The resolution a page is sized for when no OCR configuration says: an
/// image pixel is a page pixel when rendered at this many dots per inch.
pub const DEFAULT_DPI: u16 = 150;

/// The image formats an import reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Jpeg,
    Png,
    WebP,
    Gif,
    Tiff,
    /// HEIC or HEIF, decoded by the operating system.
    Heic,
}

impl ImageFormat {
    /// The format `bytes` start with, from its signature.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            Some(Self::Jpeg)
        } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some(Self::Gif)
        } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            Some(Self::WebP)
        } else if [b"II*\0", b"MM\0*", b"II+\0", b"MM\0+"]
            .iter()
            .any(|magic| bytes.starts_with(*magic))
        {
            // Classic TIFF and BigTIFF, in either byte order.
            Some(Self::Tiff)
        } else if heic::is_heic(bytes) {
            Some(Self::Heic)
        } else {
            None
        }
    }

    /// The format a file extension names (case-insensitive, without the dot).
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" | "jpe" | "jfif" => Some(Self::Jpeg),
            "png" => Some(Self::Png),
            "webp" => Some(Self::WebP),
            "gif" => Some(Self::Gif),
            "tif" | "tiff" => Some(Self::Tiff),
            "heic" | "heif" | "hif" => Some(Self::Heic),
            _ => None,
        }
    }

    /// The format `path`'s extension names.
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|extension| extension.to_str())
            .and_then(Self::from_extension)
    }

    /// The `image` decoder's format; TIFF has its own reader ([`tiff`]).
    fn image_format(self) -> Option<image::ImageFormat> {
        match self {
            Self::Jpeg => Some(image::ImageFormat::Jpeg),
            Self::Png => Some(image::ImageFormat::Png),
            Self::WebP => Some(image::ImageFormat::WebP),
            Self::Gif => Some(image::ImageFormat::Gif),
            Self::Tiff | Self::Heic => None,
        }
    }

    /// A photo is stored as a JPEG again; a lossless source with Flate.
    fn is_photo(self) -> bool {
        matches!(self, Self::Jpeg | Self::WebP | Self::Heic)
    }
}

/// Why an image could not become a PDF.
#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    /// Larger than [`limits`] allow to decode.
    #[error("This image is too large to import.")]
    TooLarge,
    /// Not an image mdoc reads, damaged, or in a variant the decoder lacks.
    #[error("This image is damaged or in a form mdoc can't read.")]
    Unreadable,
    /// A HEIC photo on a Windows PC without the extensions that decode it.
    #[error("{}", missing_codec_message(.0))]
    HeicCodecMissing(heic::MissingCodec),
    /// A HEIC photo where no decoder for it is built in.
    #[error("This computer can't decode HEIC photos. Save the photo as JPEG and import that.")]
    HeicUnsupported,
}

/// The card's words for the Windows extensions a HEIC photo needs.
fn missing_codec_message(missing: &heic::MissingCodec) -> &'static str {
    match missing {
        heic::MissingCodec::HeifExtension => {
            "Opening HEIC photos on Windows needs Microsoft's free HEIF Image Extensions \
             and HEVC Video Extensions. Install them from the Microsoft Store, then import \
             the photo again."
        }
        heic::MissingCodec::HevcExtension => {
            "Opening HEIC photos on Windows needs Microsoft's HEVC Video Extensions. \
             Install them from the Microsoft Store, then import the photo again."
        }
    }
}

/// A PDF built from images, one page each.
#[derive(Clone, Debug)]
pub struct ImagePdf {
    pub pdf: Vec<u8>,
    pub pages: u32,
    /// Frames of animated images after their first, which the PDF leaves
    /// out.
    pub frames_left_out: u64,
}

/// A PDF with one page per image in `images`, in order, each sized so that
/// an image pixel is a page pixel when rendered at `dpi` (OCR's render
/// resolution). The images are read from memory and the PDF is built in
/// memory.
pub fn build(images: &[&[u8]], dpi: u16) -> Result<ImagePdf, ImageError> {
    let mut built = Vec::with_capacity(images.len());
    let mut frames_left_out = 0u64;
    for bytes in images {
        let format = ImageFormat::from_bytes(bytes).ok_or(ImageError::Unreadable)?;
        frames_left_out += later_frames(format, bytes);
        built.extend(pages(format, bytes)?);
    }
    let pages = u32::try_from(built.len()).unwrap_or(u32::MAX);
    Ok(ImagePdf {
        pdf: pdf::write(&built, dpi),
        pages,
        frames_left_out,
    })
}

/// The decoder limits: dimensions and memory a decode may claim before any
/// pixel is decoded, so a small file can't claim gigabytes.
pub fn limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_ALLOC);
    limits
}

/// The longest side a decode accepts (a scanned panorama fits).
const MAX_DIMENSION: u32 = 30_000;

/// The memory a decode may allocate: a 100-megapixel RGBA image.
const MAX_ALLOC: u64 = 512 * 1024 * 1024;

/// How a page's pixels are stored in the PDF.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stream {
    /// A JPEG, decoded by the reader (`DCTDecode`).
    Jpeg,
    /// Raw samples, zlib-compressed (`FlateDecode`).
    Flate,
    /// CCITT fax data as the file holds it (`CCITTFaxDecode`).
    Ccitt(Ccitt),
}

/// A CCITT stream's decoding parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ccitt {
    /// 0 for Group 3 1D, positive for Group 3 2D, -1 for Group 4.
    k: i32,
    /// Black pixels are the coded 1 bits (a min-is-black TIFF).
    black_is_1: bool,
    /// Each coded row starts on a byte boundary.
    byte_align: bool,
}

/// One page: an image and how to show it upright.
#[derive(Clone, Debug)]
struct Page {
    /// Stored size, in pixels.
    width: u32,
    height: u32,
    /// One component (gray) or three (RGB).
    gray: bool,
    /// Bits per sample: 8, or 1 for a bilevel (fax) page.
    bits: u8,
    stream: Stream,
    data: Vec<u8>,
    /// EXIF orientation, 1 to 8.
    orientation: u8,
    /// The page's own horizontal and vertical resolution, in dots per inch,
    /// when it states one (a TIFF); else the PDF's.
    resolution: Option<(f64, f64)>,
}

/// The pages `bytes` give as `format`: the seam formats plug into.
fn pages(format: ImageFormat, bytes: &[u8]) -> Result<Vec<Page>, ImageError> {
    if format == ImageFormat::Tiff {
        return tiff::pages(bytes);
    }
    if format == ImageFormat::Heic {
        return Ok(vec![heic::page(bytes)?]);
    }
    if format == ImageFormat::Jpeg
        && let Some(page) = jpeg::passthrough(bytes)
    {
        return Ok(vec![page]);
    }
    let (image, orientation) = decode(format, bytes)?;
    Ok(vec![encode(image, orientation, format.is_photo())?])
}

/// The first frame of `bytes`, within [`limits`], and its EXIF orientation.
fn decode(format: ImageFormat, bytes: &[u8]) -> Result<(image::DynamicImage, u8), ImageError> {
    use image::ImageDecoder;
    let format = format.image_format().ok_or(ImageError::Unreadable)?;
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits());
    let mut decoder = reader.into_decoder().map_err(error)?;
    // Not every decoder charges its output buffer to the limits, so the
    // pixels' size is checked here, before any is decoded.
    if decoder.total_bytes() > MAX_ALLOC {
        return Err(ImageError::TooLarge);
    }
    let orientation = decoder
        .orientation()
        .map_or(1, |orientation| orientation.to_exif());
    let image = image::DynamicImage::from_decoder(decoder).map_err(error)?;
    Ok((image, orientation))
}

fn error(error: image::ImageError) -> ImageError {
    match error {
        image::ImageError::Limits(_) => ImageError::TooLarge,
        _ => ImageError::Unreadable,
    }
}

/// `image` as a page: on white, 8-bit, scaled down to [`MAX_LONG_SIDE`],
/// stored as a JPEG when it is a `photo`, else with Flate.
fn encode(image: image::DynamicImage, orientation: u8, photo: bool) -> Result<Page, ImageError> {
    let gray = !image.color().has_color();
    let image = scale_down(on_white(image, gray));
    let (width, height) = (image.width(), image.height());
    let (stream, data) = if photo {
        let mut jpeg = Vec::new();
        image
            .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut jpeg,
                JPEG_QUALITY,
            ))
            .map_err(error)?;
        (Stream::Jpeg, jpeg)
    } else {
        (Stream::Flate, deflate(image.as_bytes()))
    };
    Ok(Page {
        width,
        height,
        gray,
        bits: 8,
        stream,
        data,
        orientation,
        resolution: None,
    })
}

/// `image` as 8-bit gray or RGB, its transparency composited on white.
fn on_white(image: image::DynamicImage, gray: bool) -> image::DynamicImage {
    use image::DynamicImage;
    let blend = |value: u8, alpha: u8| -> u8 {
        let (value, alpha) = (u32::from(value), u32::from(alpha));
        ((value * alpha + 255 * (255 - alpha) + 127) / 255) as u8
    };
    match (gray, image.color().has_alpha()) {
        (true, false) => DynamicImage::ImageLuma8(image.into_luma8()),
        (false, false) => DynamicImage::ImageRgb8(image.into_rgb8()),
        (true, true) => {
            let source = image.into_luma_alpha8();
            let (width, height) = source.dimensions();
            let pixels = source
                .pixels()
                .map(|pixel| blend(pixel[0], pixel[1]))
                .collect();
            DynamicImage::ImageLuma8(
                image::GrayImage::from_raw(width, height, pixels).expect("one sample per pixel"),
            )
        }
        (false, true) => {
            let source = image.into_rgba8();
            let (width, height) = source.dimensions();
            let pixels = source
                .pixels()
                .flat_map(|pixel| [0, 1, 2].map(|channel| blend(pixel[channel], pixel[3])))
                .collect();
            DynamicImage::ImageRgb8(
                image::RgbImage::from_raw(width, height, pixels).expect("three samples per pixel"),
            )
        }
    }
}

/// `image` with its long side at most [`MAX_LONG_SIDE`], its aspect kept:
/// area averaging, which suits a downscale and costs half of a cubic filter.
fn scale_down(image: image::DynamicImage) -> image::DynamicImage {
    if image.width().max(image.height()) <= MAX_LONG_SIDE {
        return image;
    }
    image.thumbnail(MAX_LONG_SIDE, MAX_LONG_SIDE)
}

/// `samples` zlib-compressed, as `FlateDecode` reads them. Fast compression:
/// the PDF lives in memory for one import, so speed beats size.
fn deflate(samples: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder
        .write_all(samples)
        .expect("writing to memory doesn't fail");
    encoder.finish().expect("writing to memory doesn't fail")
}

/// Frames after the first in an animated GIF or WebP, read from the file's
/// structure without decoding a frame; 0 for a still image.
fn later_frames(format: ImageFormat, bytes: &[u8]) -> u64 {
    let frames = match format {
        ImageFormat::Gif => gif_frames(bytes),
        ImageFormat::WebP => webp_frames(bytes),
        // A TIFF's pages are all imported: none is a left-out frame.
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::Tiff | ImageFormat::Heic => 1,
    };
    frames.saturating_sub(1)
}

/// The image descriptors in a GIF: its frames.
fn gif_frames(bytes: &[u8]) -> u64 {
    // Header and logical screen descriptor, then the global colour table.
    let Some(&flags) = bytes.get(10) else {
        return 1;
    };
    let mut at = 13 + color_table(flags);
    let mut frames = 0u64;
    let skip_blocks = |mut at: usize| -> Option<usize> {
        loop {
            let length = usize::from(*bytes.get(at)?);
            at += 1 + length;
            if length == 0 {
                return Some(at);
            }
        }
    };
    loop {
        match bytes.get(at) {
            // Extension: its label, then data sub-blocks.
            Some(0x21) => match skip_blocks(at + 2) {
                Some(next) => at = next,
                None => break,
            },
            // Image descriptor, local colour table, LZW code size, data.
            Some(0x2c) => {
                frames += 1;
                let Some(&flags) = bytes.get(at + 9) else {
                    break;
                };
                match skip_blocks(at + 10 + color_table(flags) + 1) {
                    Some(next) => at = next,
                    None => break,
                }
            }
            _ => break,
        }
    }
    frames.max(1)
}

/// The size of the colour table a GIF descriptor's `flags` announce.
fn color_table(flags: u8) -> usize {
    if flags & 0x80 == 0 {
        0
    } else {
        3 << ((flags & 0x07) + 1)
    }
}

/// The `ANMF` chunks in a WebP: its frames, or 1 for a still image.
fn webp_frames(bytes: &[u8]) -> u64 {
    let mut at = 12;
    let mut frames = 0u64;
    while let Some(header) = bytes.get(at..at + 8) {
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        if &header[..4] == b"ANMF" {
            frames += 1;
        }
        at = at.saturating_add(8).saturating_add(size + (size & 1));
    }
    frames.max(1)
}
