//! HEIC on Windows: WIC decodes the photo with Microsoft's HEIF Image
//! Extensions, which hand the HEVC data to the HEVC Video Extensions.
//! The pixels come out as stored, 8-bit RGBA with straight alpha (a HEIF
//! may carry an alpha plane, which the extension leaves out and this module
//! decodes on its own; the builder puts it on white), with the EXIF
//! orientation (`System.Photo.Orientation`) that shows them upright; the
//! builder turns them. Only pixels leave this module.
//!
//! Without the HEIF extension WIC has no HEIF decoder and says
//! `WINCODEC_ERR_COMPONENTNOTFOUND` when one is asked for. With it but
//! without HEVC, the decoder is found and decoding the pixels fails for
//! want of a codec.
#![allow(unsafe_code)] // COM calls into WIC (#897).

use super::{ImageError, MissingCodec, OsDecoded, OsDecoder};
use windows::{
    Win32::{
        Foundation::{RPC_E_CHANGED_MODE, WINCODEC_ERR_COMPONENTNOTFOUND},
        Graphics::Imaging::{
            CLSID_WICImagingFactory, GUID_ContainerFormatHeif, GUID_WICPixelFormat32bppRGBA,
            IWICBitmapFrameDecode, IWICImagingFactory, WICBitmapDitherTypeNone,
            WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
        },
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
                StructuredStorage::{PROPVARIANT, PropVariantClear},
            },
            Variant::VT_UI2,
        },
    },
    core::{GUID, HRESULT, w},
};

/// Media Foundation's "no codec for this format", which the HEIF decoder
/// passes on when the HEVC extension isn't there.
const MF_E_TOPO_CODEC_NOT_FOUND: HRESULT = HRESULT(0xC00D_5212_u32 as i32);

pub(super) struct Decoder;

/// The frame's EXIF orientation (`System.Photo.Orientation`), 1 when it
/// has none.
fn orientation(frame: &IWICBitmapFrameDecode) -> u8 {
    // SAFETY: the value is read only when WIC filled it as a VT_UI2, and
    // cleared whatever it holds before it goes.
    unsafe {
        let Ok(reader) = frame.GetMetadataQueryReader() else {
            return 1;
        };
        let mut value = PROPVARIANT::default();
        let mut orientation = 1;
        if reader
            .GetMetadataByName(w!("System.Photo.Orientation"), &mut value)
            .is_ok()
            && value.Anonymous.Anonymous.vt == VT_UI2
        {
            let read = value.Anonymous.Anonymous.Anonymous.uiVal;
            if (1..=8).contains(&read) {
                orientation = read as u8;
            }
        }
        let _ = PropVariantClear(&mut value);
        orientation
    }
}

impl OsDecoder for Decoder {
    fn decode_heic(&self, bytes: &[u8]) -> Result<OsDecoded, ImageError> {
        let mut decoded = decode(bytes, &GUID_ContainerFormatHeif)?;
        // Windows' HEIF extension decodes the primary image without its
        // alpha plane (its frames are 32bppBGR). The plane is an image of
        // its own in the file: named primary in a copy, it decodes the same
        // way, its gray the alpha. A plane that doesn't decode, or doesn't
        // match, leaves the photo opaque.
        if let Some(alpha) =
            super::alpha_item(bytes).and_then(|item| super::with_primary(bytes, item))
            && let image::DynamicImage::ImageRgba8(pixels) = &mut decoded.image
            && pixels.pixels().all(|pixel| pixel[3] == u8::MAX)
            && let Ok(plane) = decode(&alpha, &GUID_ContainerFormatHeif)
        {
            let plane = plane.image.to_luma8();
            if plane.dimensions() == pixels.dimensions() {
                for (pixel, alpha) in pixels.pixels_mut().zip(plane.pixels()) {
                    pixel[3] = alpha[0];
                }
            }
        }
        Ok(decoded)
    }
}

/// COM for this thread, for as long as the guard lives.
struct Com {
    initialized: bool,
}

impl Com {
    fn enter() -> Result<Self, ImageError> {
        // SAFETY: called on this thread, balanced by `Drop` when it
        // succeeded; a thread already in another apartment keeps it.
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result == RPC_E_CHANGED_MODE {
            return Ok(Self { initialized: false });
        }
        result.ok().map_err(|_| ImageError::Unreadable)?;
        Ok(Self { initialized: true })
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: balances this thread's successful CoInitializeEx.
            unsafe { CoUninitialize() }
        }
    }
}

/// `bytes` decoded by WIC's decoder for the `container` format: HEIF in
/// use, one no decoder implements in the test of the missing-extension
/// path. COM objects are released as they drop, before `_com` leaves COM.
pub(super) fn decode(bytes: &[u8], container: &GUID) -> Result<OsDecoded, ImageError> {
    // WIC takes the stream's length as 32 bits.
    if u32::try_from(bytes.len()).is_err() {
        return Err(ImageError::TooLarge);
    }
    let _com = Com::enter()?;
    let unreadable = |_| ImageError::Unreadable;
    // SAFETY: plain COM calls on interfaces WIC returned; the stream reads
    // `bytes`, which outlive every interface here (dropped at the end of
    // this block); `pixels` is exactly `height` rows of `stride` bytes.
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(unreadable)?;
        let stream = factory.CreateStream().map_err(unreadable)?;
        stream.InitializeFromMemory(bytes).map_err(unreadable)?;
        // Asking for the HEIF decoder by its container format is asking
        // whether the HEIF extension is installed.
        let decoder = factory
            .CreateDecoder(container, std::ptr::null())
            .map_err(|error| {
                if error.code() == WINCODEC_ERR_COMPONENTNOTFOUND {
                    ImageError::HeicCodecMissing(MissingCodec::HeifExtension)
                } else {
                    ImageError::Unreadable
                }
            })?;
        decoder
            .Initialize(&stream, WICDecodeMetadataCacheOnDemand)
            .map_err(unreadable)?;
        let missing_hevc = |error: windows::core::Error| {
            if error.code() == WINCODEC_ERR_COMPONENTNOTFOUND
                || error.code() == MF_E_TOPO_CODEC_NOT_FOUND
            {
                ImageError::HeicCodecMissing(MissingCodec::HevcExtension)
            } else {
                ImageError::Unreadable
            }
        };
        let frame = decoder.GetFrame(0).map_err(missing_hevc)?;
        let orientation = orientation(&frame);
        let converter = factory.CreateFormatConverter().map_err(unreadable)?;
        converter
            .Initialize(
                &frame,
                &GUID_WICPixelFormat32bppRGBA,
                WICBitmapDitherTypeNone,
                None,
                0.,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(missing_hevc)?;
        let (mut width, mut height) = (0u32, 0u32);
        converter
            .GetSize(&mut width, &mut height)
            .map_err(unreadable)?;
        if width == 0 || height == 0 {
            return Err(ImageError::Unreadable);
        }
        if width > super::super::MAX_DIMENSION || height > super::super::MAX_DIMENSION {
            return Err(ImageError::TooLarge);
        }
        let stride = width.checked_mul(4).ok_or(ImageError::TooLarge)?;
        let size = u64::from(stride) * u64::from(height);
        if size > super::MAX_ALLOC {
            return Err(ImageError::TooLarge);
        }
        let mut pixels = vec![0u8; size as usize];
        converter
            .CopyPixels(std::ptr::null(), stride, &mut pixels)
            .map_err(missing_hevc)?;
        let image =
            image::RgbaImage::from_raw(width, height, pixels).ok_or(ImageError::Unreadable)?;
        Ok(OsDecoded {
            image: image::DynamicImage::ImageRgba8(image),
            orientation,
        })
    }
}
