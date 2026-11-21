//! HEIC on macOS: ImageIO decodes the file's primary image and applies its
//! orientation (`kCGImageSourceCreateThumbnailWithTransform`, at full
//! size), and CoreGraphics draws it into an 8-bit RGB bitmap on white.
//! The image's size is read from its properties first and held to the
//! builder's limits, and the decode is bounded to it. Only pixels leave
//! this module.
#![allow(unsafe_code)] // ImageIO, CoreGraphics and CoreFoundation calls (#897).

use super::{ImageError, OsDecoded, OsDecoder};
use std::ffi::c_void;

type CFTypeRef = *const c_void;
type CFIndex = isize;

#[repr(C)]
struct CGRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFBooleanTrue: CFTypeRef;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    fn CFDataCreate(allocator: CFTypeRef, bytes: *const u8, length: CFIndex) -> CFTypeRef;
    fn CFDictionaryCreate(
        allocator: CFTypeRef,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        count: CFIndex,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFTypeRef;
    fn CFRelease(value: CFTypeRef);
    fn CFDictionaryGetValue(dictionary: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
    fn CFNumberCreate(allocator: CFTypeRef, kind: CFIndex, value: *const c_void) -> CFTypeRef;
    fn CFNumberGetValue(number: CFTypeRef, kind: CFIndex, value: *mut c_void) -> u8;
}

/// `kCFNumberSInt64Type`.
const SINT64: CFIndex = 4;

#[link(name = "ImageIO", kind = "framework")]
unsafe extern "C" {
    static kCGImageSourceCreateThumbnailFromImageAlways: CFTypeRef;
    static kCGImageSourceCreateThumbnailWithTransform: CFTypeRef;
    static kCGImageSourceThumbnailMaxPixelSize: CFTypeRef;
    static kCGImagePropertyPixelWidth: CFTypeRef;
    static kCGImagePropertyPixelHeight: CFTypeRef;
    fn CGImageSourceCreateWithData(data: CFTypeRef, options: CFTypeRef) -> CFTypeRef;
    fn CGImageSourceGetCount(source: CFTypeRef) -> usize;
    fn CGImageSourceGetPrimaryImageIndex(source: CFTypeRef) -> usize;
    fn CGImageSourceCopyPropertiesAtIndex(
        source: CFTypeRef,
        index: usize,
        options: CFTypeRef,
    ) -> CFTypeRef;
    fn CGImageSourceCreateThumbnailAtIndex(
        source: CFTypeRef,
        index: usize,
        options: CFTypeRef,
    ) -> CFTypeRef;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGImageGetWidth(image: CFTypeRef) -> usize;
    fn CGImageGetHeight(image: CFTypeRef) -> usize;
    fn CGColorSpaceCreateDeviceRGB() -> CFTypeRef;
    fn CGBitmapContextCreate(
        data: *mut c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        space: CFTypeRef,
        bitmap_info: u32,
    ) -> CFTypeRef;
    fn CGContextDrawImage(context: CFTypeRef, rect: CGRect, image: CFTypeRef);
}

/// `kCGImageAlphaNoneSkipLast`: RGBX, 8 bits a component.
const RGBX: u32 = 5;

/// A Core Foundation object released when dropped.
struct Owned(CFTypeRef);

impl Owned {
    fn new(value: CFTypeRef) -> Result<Self, ImageError> {
        if value.is_null() {
            Err(ImageError::Unreadable)
        } else {
            Ok(Self(value))
        }
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a non-null object this module created and
        // owns one reference to.
        unsafe { CFRelease(self.0) }
    }
}

pub(super) struct Decoder;

impl OsDecoder for Decoder {
    fn decode_heic(&self, bytes: &[u8]) -> Result<OsDecoded, ImageError> {
        let length = CFIndex::try_from(bytes.len()).map_err(|_| ImageError::TooLarge)?;
        // SAFETY: every Create and Copy call's result is checked for null
        // and released by `Owned`; the option keys and values are ImageIO's
        // and CoreFoundation's own constants or numbers made here; the
        // dictionary's values are read only while `properties` holds them,
        // and a number is read into an i64 as kCFNumberSInt64Type; the
        // bitmap context draws into `pixels`, which outlives it and is
        // exactly `height` rows of `width * 4` bytes.
        unsafe {
            let data = Owned::new(CFDataCreate(std::ptr::null(), bytes.as_ptr(), length))?;
            let source = Owned::new(CGImageSourceCreateWithData(data.0, std::ptr::null()))?;
            // The image the file names primary, which need not be its
            // first.
            let index = CGImageSourceGetPrimaryImageIndex(source.0);
            if index >= CGImageSourceGetCount(source.0) {
                return Err(ImageError::Unreadable);
            }
            // Its size as stored, from its properties, before any pixel is
            // decoded: a small file can claim a huge image.
            let properties = Owned::new(CGImageSourceCopyPropertiesAtIndex(
                source.0,
                index,
                std::ptr::null(),
            ))?;
            let side = |key: CFTypeRef| -> Option<u32> {
                let number = CFDictionaryGetValue(properties.0, key);
                if number.is_null() {
                    return None;
                }
                let mut value = 0i64;
                (CFNumberGetValue(number, SINT64, (&raw mut value).cast()) != 0)
                    .then_some(value)
                    .and_then(|value| u32::try_from(value).ok())
                    .filter(|&value| value > 0)
            };
            let (stored_width, stored_height) = (
                side(kCGImagePropertyPixelWidth).ok_or(ImageError::Unreadable)?,
                side(kCGImagePropertyPixelHeight).ok_or(ImageError::Unreadable)?,
            );
            super::fits(stored_width, stored_height)?;
            // The whole image, never an embedded preview, turned upright,
            // and no larger than it is stored.
            let longest = i64::from(stored_width.max(stored_height));
            let longest = Owned::new(CFNumberCreate(
                std::ptr::null(),
                SINT64,
                (&raw const longest).cast(),
            ))?;
            let keys = [
                kCGImageSourceCreateThumbnailFromImageAlways,
                kCGImageSourceCreateThumbnailWithTransform,
                kCGImageSourceThumbnailMaxPixelSize,
            ];
            let values = [kCFBooleanTrue, kCFBooleanTrue, longest.0];
            let options = Owned::new(CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                3,
                &raw const kCFTypeDictionaryKeyCallBacks,
                &raw const kCFTypeDictionaryValueCallBacks,
            ))?;
            let image = Owned::new(CGImageSourceCreateThumbnailAtIndex(
                source.0, index, options.0,
            ))?;
            let (width, height) = (CGImageGetWidth(image.0), CGImageGetHeight(image.0));
            if width == 0 || height == 0 {
                return Err(ImageError::Unreadable);
            }
            let size = width
                .checked_mul(height)
                .and_then(|pixels| pixels.checked_mul(4))
                .filter(|&size| size as u64 <= super::MAX_ALLOC)
                .ok_or(ImageError::TooLarge)?;
            // White underneath, for a photo with transparency.
            let mut pixels = vec![0xffu8; size];
            let space = Owned::new(CGColorSpaceCreateDeviceRGB())?;
            let context = Owned::new(CGBitmapContextCreate(
                pixels.as_mut_ptr().cast(),
                width,
                height,
                8,
                width * 4,
                space.0,
                RGBX,
            ))?;
            let rect = CGRect {
                x: 0.,
                y: 0.,
                width: width as f64,
                height: height as f64,
            };
            CGContextDrawImage(context.0, rect, image.0);
            drop(context);
            let rgb = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
                .collect();
            let width = u32::try_from(width).map_err(|_| ImageError::TooLarge)?;
            let height = u32::try_from(height).map_err(|_| ImageError::TooLarge)?;
            let image =
                image::RgbImage::from_raw(width, height, rgb).ok_or(ImageError::Unreadable)?;
            Ok(OsDecoded {
                image: image::DynamicImage::ImageRgb8(image),
                orientation: 1,
            })
        }
    }
}
