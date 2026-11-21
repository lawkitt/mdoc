//! TIFF: every page (image file directory), each sized by its own
//! resolution and turned upright by its Orientation tag.
//!
//! - **CCITT Group 3 and Group 4** pages pass through as `CCITTFaxDecode`
//!   (#1024 decision 12): the file's coded strips, never decoded, with `K`
//!   0 (G3 1D), positive (G3 2D) or -1 (G4), `BlackIs1` from the
//!   photometric interpretation and `EncodedByteAlign` from T4Options bit 2.
//!   A page with fill order 2 has its bytes bit-reversed first.
//! - **Several strips** on a CCITT page: Group 3 strips are joined when
//!   every strip after the first starts with an end-of-line code followed
//!   by a one-dimensional row, which is how libtiff writes them. The joined
//!   stream then decodes as one: each row is found by its end-of-line
//!   code, and no row refers to a row of another strip. Otherwise each
//!   strip is decoded on its own (`hayro-ccitt`) and the page is stored
//!   losslessly as 1-bit Flate. Group 4 strips are never joined: each
//!   strip codes its first row against an imaginary white row, the joined
//!   stream would code it against the real row above.
//! - **Everything else** (uncompressed, PackBits, LZW, Deflate, JPEG) is
//!   decoded by the `tiff` crate within the builder's limits, then follows
//!   the builder's rules: bilevel pages stay 1-bit (Flate), others become
//!   8-bit gray or RGB on white, scaled to the long-side cap, with Flate.
//! - No tag but the image's own (size, resolution, orientation, coding)
//!   is read into the PDF: descriptions, software, dates and the like stay
//!   out.

use super::{Ccitt, ImageError, MAX_ALLOC, MAX_DIMENSION, MAX_LONG_SIDE, Page, Stream};
use std::io::Cursor;
use tiff::{
    ColorType, TiffError,
    decoder::{Decoder, DecodingResult, ifd::Value},
    tags::Tag,
};

/// More directories than any document's pages: a bound on a file whose
/// directories loop or never end.
const MAX_PAGES: usize = 5_000;

/// T4Options and T6Options, which the `tiff` crate doesn't name.
const T4_OPTIONS: u16 = 292;
const T6_OPTIONS: u16 = 293;

/// Every page of the TIFF in `bytes`, in order.
pub(super) fn pages(bytes: &[u8]) -> Result<Vec<Page>, ImageError> {
    let mut decoder = Decoder::new(Cursor::new(bytes))
        .map_err(error)?
        .with_limits(limits());
    let mut pages = Vec::new();
    loop {
        if pages.len() == MAX_PAGES {
            return Err(ImageError::TooLarge);
        }
        pages.push(page(&mut decoder, bytes)?);
        if !decoder.more_images() {
            return Ok(pages);
        }
        decoder.next_image().map_err(error)?;
    }
}

fn limits() -> tiff::decoder::Limits {
    let mut limits = tiff::decoder::Limits::default();
    limits.decoding_buffer_size = MAX_ALLOC as usize;
    limits
}

fn error(error: TiffError) -> ImageError {
    match error {
        TiffError::LimitsExceeded => ImageError::TooLarge,
        _ => ImageError::Unreadable,
    }
}

type TiffDecoder<'a> = Decoder<Cursor<&'a [u8]>>;

/// The current directory as a page.
fn page(decoder: &mut TiffDecoder, bytes: &[u8]) -> Result<Page, ImageError> {
    let (width, height) = decoder.dimensions().map_err(error)?;
    if width == 0 || height == 0 {
        return Err(ImageError::Unreadable);
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(ImageError::TooLarge);
    }
    let orientation = tag::<u8>(decoder, Tag::Orientation)
        .filter(|orientation| (1..=8).contains(orientation))
        .unwrap_or(1);
    let resolution = resolution(decoder);
    let compression = tag::<u16>(decoder, Tag::Compression).unwrap_or(1);
    let page = match compression {
        3 | 4 => match fax(decoder, bytes, width, height, compression)? {
            Some(page) => page,
            None => decoded(decoder, width, height)?,
        },
        _ => decoded(decoder, width, height)?,
    };
    Ok(Page {
        orientation,
        resolution,
        ..page
    })
}

fn tag<T: TryFrom<u64>>(decoder: &mut TiffDecoder, tag: Tag) -> Option<T> {
    decoder.find_tag_unsigned(tag).ok().flatten()
}

/// The page's horizontal and vertical resolution in dots per inch; `None`
/// when it states none, so the PDF's applies. A resolution without a unit
/// keeps only its aspect.
fn resolution(decoder: &mut TiffDecoder) -> Option<(f64, f64)> {
    let mut read = |tag| {
        decoder
            .find_tag(tag)
            .ok()
            .flatten()
            .and_then(|value| match value {
                Value::Rational(numerator, denominator) if denominator > 0 => {
                    Some(f64::from(numerator) / f64::from(denominator))
                }
                Value::Float(value) => Some(f64::from(value)),
                Value::Double(value) => Some(value),
                Value::Short(value) => Some(f64::from(value)),
                Value::Unsigned(value) => Some(f64::from(value)),
                _ => None,
            })
            .filter(|value: &f64| value.is_finite() && *value > 0.)
    };
    let (x, y) = (
        read(Tag::XResolution)?,
        read(Tag::YResolution).unwrap_or(f64::NAN),
    );
    let y = if y.is_nan() { x } else { y };
    let unit = tag::<u16>(decoder, Tag::ResolutionUnit).unwrap_or(2);
    let (x, y) = match unit {
        // Centimetres.
        3 => (x * 2.54, y * 2.54),
        // No unit: the aspect only, against the PDF's resolution.
        1 => {
            let base = f64::from(super::DEFAULT_DPI);
            (base, base * y / x)
        }
        _ => (x, y),
    };
    // A resolution no scanner or fax uses is a broken tag.
    let sane = |value: f64| (10.0..=10_000.0).contains(&value);
    (sane(x) && sane(y)).then_some((x, y))
}

/// A CCITT page, passed through, or decoded strip by strip when its strips
/// can't be joined; `None` when the `tiff` crate must decode it instead
/// (tiles, Group 4 uncompressed mode).
fn fax(
    decoder: &mut TiffDecoder,
    bytes: &[u8],
    width: u32,
    height: u32,
    compression: u16,
) -> Result<Option<Page>, ImageError> {
    let bits = tag::<u16>(decoder, Tag::BitsPerSample).unwrap_or(1);
    let samples = tag::<u16>(decoder, Tag::SamplesPerPixel).unwrap_or(1);
    if bits != 1 || samples != 1 || decoder.find_tag(Tag::TileOffsets).ok().flatten().is_some() {
        return Ok(None);
    }
    let (k, byte_align) = if compression == 3 {
        let options = tag::<u32>(decoder, Tag::Unknown(T4_OPTIONS)).unwrap_or(0);
        // Uncompressed mode (bit 1) isn't a PDF filter option.
        if options & 2 != 0 {
            return Err(ImageError::Unreadable);
        }
        let k = if options & 1 != 0 {
            i32::try_from(height).unwrap_or(i32::MAX)
        } else {
            0
        };
        (k, options & 4 != 0)
    } else {
        let options = tag::<u32>(decoder, Tag::Unknown(T6_OPTIONS)).unwrap_or(0);
        if options & 2 != 0 {
            return Ok(None);
        }
        (-1, false)
    };
    let photometric = tag::<u16>(decoder, Tag::PhotometricInterpretation).unwrap_or(0);
    let reversed = tag::<u16>(decoder, Tag::FillOrder) == Some(2);
    let offsets: Vec<u64> = decoder
        .find_tag_unsigned_vec(Tag::StripOffsets)
        .ok()
        .flatten()
        .ok_or(ImageError::Unreadable)?;
    let counts: Vec<u64> = decoder
        .find_tag_unsigned_vec(Tag::StripByteCounts)
        .ok()
        .flatten()
        .ok_or(ImageError::Unreadable)?;
    if offsets.is_empty() || offsets.len() != counts.len() {
        return Err(ImageError::Unreadable);
    }
    let mut strips = Vec::with_capacity(offsets.len());
    for (&offset, &count) in offsets.iter().zip(&counts) {
        let start = usize::try_from(offset).map_err(|_| ImageError::Unreadable)?;
        let end = start
            .checked_add(usize::try_from(count).map_err(|_| ImageError::Unreadable)?)
            .ok_or(ImageError::Unreadable)?;
        let strip = bytes.get(start..end).ok_or(ImageError::Unreadable)?;
        strips.push(if reversed {
            strip
                .iter()
                .map(|byte| byte.reverse_bits())
                .collect::<Vec<u8>>()
        } else {
            strip.to_vec()
        });
    }
    let joinable = strips.len() == 1
        || (k >= 0
            && strips[1..]
                .iter()
                .all(|strip| starts_a_fresh_row(strip, k > 0)));
    if !joinable {
        let rows_per_strip = tag::<u32>(decoder, Tag::RowsPerStrip).unwrap_or(height);
        let mut packed = decode_strips(&strips, width, height, rows_per_strip, k, byte_align)?;
        // A min-is-black page's coded white runs show black.
        if photometric == 1 {
            packed.iter_mut().for_each(|byte| *byte = !*byte);
        }
        return Ok(Some(Page {
            width,
            height,
            gray: true,
            bits: 1,
            stream: Stream::Flate,
            data: super::deflate(&packed),
            orientation: 1,
            resolution: None,
            sheet: (width, height),
        }));
    }
    let data = strips.concat();
    Ok(Some(Page {
        width,
        height,
        gray: true,
        bits: 1,
        stream: Stream::Ccitt(Ccitt {
            k,
            // A min-is-black page's coded white runs show black.
            black_is_1: photometric == 1,
            byte_align,
        }),
        data,
        orientation: 1,
        resolution: None,
        sheet: (width, height),
    }))
}

/// Whether Group 3 `data` (bits most significant first) opens with an
/// end-of-line code, fill allowed before it, and, in two-dimensional
/// coding, a one-dimensional row after it: a strip that can follow
/// another in one stream.
fn starts_a_fresh_row(data: &[u8], two_dimensional: bool) -> bool {
    let bit = |index: usize| data.get(index / 8).map(|byte| byte >> (7 - index % 8) & 1);
    let mut index = 0;
    while bit(index) == Some(0) {
        index += 1;
    }
    // An end-of-line code is eleven zeros and a one; fill adds zeros.
    if index < 11 || bit(index) != Some(1) {
        return false;
    }
    !two_dimensional || bit(index + 1) == Some(1)
}

/// A CCITT page's `strips` decoded one by one (`hayro-ccitt`): rows packed
/// most significant bit first, padded to a byte, 1 meaning a coded white
/// pixel. Each strip holds `rows_per_strip` rows, the last what remains;
/// each may or may not open its rows with end-of-line codes.
pub(super) fn decode_strips(
    strips: &[Vec<u8>],
    width: u32,
    height: u32,
    rows_per_strip: u32,
    k: i32,
    byte_align: bool,
) -> Result<Vec<u8>, ImageError> {
    use hayro_ccitt::{DecodeSettings, DecoderContext, EncodingMode};
    let row_bytes = width.div_ceil(8) as usize;
    let mut rows = Rows {
        width,
        row_bytes,
        packed: Vec::with_capacity(row_bytes * height as usize),
    };
    let rows_per_strip = rows_per_strip.clamp(1, height);
    for (index, strip) in strips.iter().enumerate() {
        let first = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_mul(rows_per_strip))
            .ok_or(ImageError::Unreadable)?;
        if first >= height {
            break;
        }
        let strip_rows = rows_per_strip.min(height - first);
        let settings = DecodeSettings {
            columns: width,
            rows: strip_rows,
            end_of_block: true,
            end_of_line: k >= 0 && starts_a_fresh_row(strip, false),
            rows_are_byte_aligned: byte_align,
            encoding: match k {
                0 => EncodingMode::Group3_1D,
                1.. => EncodingMode::Group3_2D,
                _ => EncodingMode::Group4,
            },
            strict: false,
        };
        let before = rows.packed.len();
        // A strip that ends badly after its last row still gave every row;
        // one that gave fewer is damaged.
        let _ = hayro_ccitt::decode(strip, &mut rows, &mut DecoderContext::new(settings));
        let wanted = before + strip_rows as usize * row_bytes;
        if rows.packed.len() < wanted {
            return Err(ImageError::Unreadable);
        }
        rows.packed.truncate(wanted);
    }
    if rows.packed.len() != row_bytes * height as usize {
        return Err(ImageError::Unreadable);
    }
    Ok(rows.packed)
}

/// Decoded CCITT rows, packed.
struct Rows {
    width: u32,
    row_bytes: usize,
    packed: Vec<u8>,
}

impl hayro_ccitt::Decoder for Rows {
    fn push_row(&mut self, changes: &[u32]) {
        let mut row = vec![0xff; self.row_bytes];
        // Each row starts white; the runs from an even change to the next
        // are black.
        for run in changes.chunks(2) {
            let start = run[0].min(self.width);
            let end = run.get(1).copied().unwrap_or(self.width).min(self.width);
            for x in start..end {
                row[x as usize / 8] &= !(0x80 >> (x % 8));
            }
        }
        self.packed.extend_from_slice(&row);
    }
}

/// The page decoded by the `tiff` crate, within the builder's limits.
fn decoded(decoder: &mut TiffDecoder, width: u32, height: u32) -> Result<Page, ImageError> {
    let color = decoder.colortype().map_err(error)?;
    let (samples, bits) = match color {
        ColorType::Gray(bits) => (1, bits),
        ColorType::GrayA(bits) => (2, bits),
        ColorType::RGB(bits) => (3, bits),
        ColorType::RGBA(bits) => (4, bits),
        ColorType::CMYK(bits) => (4, bits),
        _ => return Err(ImageError::Unreadable),
    };
    let size = u64::from(width) * u64::from(height) * samples * u64::from(bits).div_ceil(8);
    if size > MAX_ALLOC {
        return Err(ImageError::TooLarge);
    }
    // The `tiff` crate gives a white-is-zero page as black-is-zero.
    let result = decoder.read_image().map_err(error)?;
    if bits == 1 && samples == 1 {
        return bilevel(result, width, height);
    }
    let image = to_image(result, color, width, height)?;
    super::encode(image, 1, false)
}

/// A 1-bit page, rows byte-padded as the decoder gives them, stored as 1-bit
/// Flate with 1 meaning white, as the decoder gives it. A page longer than
/// the cap is scaled as 8-bit gray instead.
fn bilevel(result: DecodingResult, width: u32, height: u32) -> Result<Page, ImageError> {
    let DecodingResult::U8(mut packed) = result else {
        return Err(ImageError::Unreadable);
    };
    let row = width.div_ceil(8) as usize;
    if packed.len() < row * height as usize {
        return Err(ImageError::Unreadable);
    }
    packed.truncate(row * height as usize);
    if width.max(height) > MAX_LONG_SIDE {
        let gray = image::GrayImage::from_fn(width, height, |x, y| {
            let byte = packed[y as usize * row + x as usize / 8];
            image::Luma([if byte >> (7 - x % 8) & 1 == 1 { 255 } else { 0 }])
        });
        return super::encode(image::DynamicImage::ImageLuma8(gray), 1, false);
    }
    Ok(Page {
        width,
        height,
        gray: true,
        bits: 1,
        stream: Stream::Flate,
        data: super::deflate(&packed),
        orientation: 1,
        resolution: None,
        sheet: (width, height),
    })
}

/// Decoded samples as an image the builder's rules take. CMYK becomes RGB
/// (TIFF stores it uninverted).
fn to_image(
    result: DecodingResult,
    color: ColorType,
    width: u32,
    height: u32,
) -> Result<image::DynamicImage, ImageError> {
    use image::{DynamicImage, ImageBuffer};
    let unreadable = || ImageError::Unreadable;
    let image = match (result, color) {
        (DecodingResult::U8(samples), ColorType::Gray(8)) => DynamicImage::ImageLuma8(
            ImageBuffer::from_raw(width, height, samples).ok_or_else(unreadable)?,
        ),
        (DecodingResult::U16(samples), ColorType::Gray(16)) => DynamicImage::ImageLuma16(
            ImageBuffer::from_raw(width, height, samples).ok_or_else(unreadable)?,
        ),
        (DecodingResult::U8(samples), ColorType::GrayA(8)) => DynamicImage::ImageLumaA8(
            ImageBuffer::from_raw(width, height, samples).ok_or_else(unreadable)?,
        ),
        (DecodingResult::U16(samples), ColorType::GrayA(16)) => DynamicImage::ImageLumaA16(
            ImageBuffer::from_raw(width, height, samples).ok_or_else(unreadable)?,
        ),
        (DecodingResult::U8(samples), ColorType::RGB(8)) => DynamicImage::ImageRgb8(
            ImageBuffer::from_raw(width, height, samples).ok_or_else(unreadable)?,
        ),
        (DecodingResult::U16(samples), ColorType::RGB(16)) => DynamicImage::ImageRgb16(
            ImageBuffer::from_raw(width, height, samples).ok_or_else(unreadable)?,
        ),
        (DecodingResult::U8(samples), ColorType::RGBA(8)) => DynamicImage::ImageRgba8(
            ImageBuffer::from_raw(width, height, samples).ok_or_else(unreadable)?,
        ),
        (DecodingResult::U16(samples), ColorType::RGBA(16)) => DynamicImage::ImageRgba16(
            ImageBuffer::from_raw(width, height, samples).ok_or_else(unreadable)?,
        ),
        (DecodingResult::U8(samples), ColorType::CMYK(8)) => {
            let rgb = samples
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|cmyk| {
                    let k = 255 - u32::from(cmyk[3]);
                    [0, 1, 2].map(|channel| ((255 - u32::from(cmyk[channel])) * k / 255) as u8)
                })
                .collect();
            DynamicImage::ImageRgb8(
                ImageBuffer::from_raw(width, height, rgb).ok_or_else(unreadable)?,
            )
        }
        _ => return Err(ImageError::Unreadable),
    };
    Ok(image)
}

#[cfg(test)]
mod tests;
