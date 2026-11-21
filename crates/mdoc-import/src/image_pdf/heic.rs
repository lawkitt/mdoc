//! HEIC and HEIF (#1024 decision 13): the operating system decodes them
//! (ImageIO on macOS, WIC with Microsoft's HEIF and HEVC extensions on
//! Windows), and hands the builder 8-bit RGB pixels and their EXIF
//! orientation. The builder then treats the photo like any other: the
//! orientation turns the page, and the pixels are stored as a quality-92
//! JPEG, so none of the file's metadata (EXIF, GPS, XMP, camera, dates)
//! reaches the PDF. Nothing is written to disk.
//!
//! [`OsDecoder`] is the seam: tests put a stub in place of the platform's
//! decoder ([`with_decoder`]), so CI needs no codec.

use super::{ImageError, MAX_ALLOC};

#[cfg(test)]
mod tests;

/// What an operating system's decoder gives: the pixels as stored, and the
/// EXIF orientation that shows them upright (1 when the decoder already
/// turned them).
pub struct OsDecoded {
    pub image: image::RgbImage,
    pub orientation: u8,
}

/// Decodes the formats whose codec only the operating system carries.
pub trait OsDecoder {
    fn decode_heic(&self, bytes: &[u8]) -> Result<OsDecoded, ImageError>;
}

/// The extensions a Windows PC needs to decode HEIC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingCodec {
    /// Microsoft's HEIF Image Extensions: without them WIC has no HEIF
    /// decoder at all, so whether HEVC is there can't be told.
    HeifExtension,
    /// Microsoft's HEVC Video Extensions: the HEIF decoder is there, but it
    /// can't decode the photo's HEVC data.
    HevcExtension,
}

/// Whether `bytes` are a HEIC or HEIF image: an ISO base media file whose
/// `ftyp` box names an HEVC-coded HEIF brand. (AVIF, also HEIF, is not
/// among them.)
pub(super) fn is_heic(bytes: &[u8]) -> bool {
    const BRANDS: [&[u8; 4]; 8] = [
        b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"hevm", b"hevs",
    ];
    if bytes.len() < 16 || &bytes[4..8] != b"ftyp" {
        return false;
    }
    let size = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let end = size.clamp(16, bytes.len().min(256));
    let major: &[u8; 4] = bytes[8..12].try_into().expect("four bytes");
    // The major brand, then the compatible brands after the minor version;
    // a generic major brand (`mif1`, `msf1`) leaves it to them.
    std::iter::once(major)
        .chain(bytes[16..end].as_chunks::<4>().0)
        .any(|brand| BRANDS.contains(&brand))
}

/// `bytes` decoded by the platform's decoder (or the test's stub), as a
/// page by the builder's rules for a photo.
pub(super) fn page(bytes: &[u8]) -> Result<super::Page, ImageError> {
    let decoded = decode(bytes)?;
    let (width, height) = decoded.image.dimensions();
    if u64::from(width) * u64::from(height) * 3 > MAX_ALLOC {
        return Err(ImageError::TooLarge);
    }
    super::encode(
        image::DynamicImage::ImageRgb8(decoded.image),
        decoded.orientation,
        true,
    )
}

fn decode(bytes: &[u8]) -> Result<OsDecoded, ImageError> {
    #[cfg(test)]
    if let Some(result) =
        STUB.with(|stub| stub.borrow().as_ref().map(|stub| stub.decode_heic(bytes)))
    {
        return result;
    }
    platform::Decoder.decode_heic(bytes)
}

#[cfg(test)]
thread_local! {
    static STUB: std::cell::RefCell<Option<Box<dyn OsDecoder>>> = const { std::cell::RefCell::new(None) };
}

/// Run `run` with `decoder` in place of the platform's, on this thread.
#[cfg(test)]
pub(crate) fn with_decoder<T>(decoder: impl OsDecoder + 'static, run: impl FnOnce() -> T) -> T {
    STUB.with(|stub| *stub.borrow_mut() = Some(Box::new(decoder)));
    let result = run();
    STUB.with(|stub| *stub.borrow_mut() = None);
    result
}

#[cfg(target_os = "windows")]
mod wic;
#[cfg(target_os = "windows")]
use wic as platform;

#[cfg(target_os = "macos")]
mod image_io;
#[cfg(target_os = "macos")]
use image_io as platform;

/// Elsewhere (Linux, and the phones, whose apps decode HEIC themselves)
/// there is no decoder here.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform {
    use super::{ImageError, OsDecoded, OsDecoder};

    pub(super) struct Decoder;

    impl OsDecoder for Decoder {
        fn decode_heic(&self, _: &[u8]) -> Result<OsDecoded, ImageError> {
            Err(ImageError::HeicUnsupported)
        }
    }
}
