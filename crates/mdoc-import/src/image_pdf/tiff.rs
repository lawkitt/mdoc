//! TIFF: every page (image file directory), each sized by its own
//! resolution and turned upright by its Orientation tag.
//!
//! - **CCITT Group 3 and Group 4** pages pass through as `CCITTFaxDecode`
//!   (#1024 decision 12): the file's coded strips, never decoded, with `K`
//!   0 (G3 1D), positive (G3 2D) or -1 (G4) and `BlackIs1` from the
//!   photometric interpretation. `EncodedByteAlign` is T4Options bit 2 only
//!   for rows without end-of-line codes: TIFF's bit aligns the end of each
//!   code, PDF's would skip into the next one. A page with fill order 2 has
//!   its bytes bit-reversed first.
//! - **Several strips** on a CCITT page: Group 3 strips are joined when
//!   every strip after the first starts with an end-of-line code followed
//!   by a one-dimensional row, which is how libtiff writes them. The joined
//!   stream then decodes as one: each row is found by its end-of-line
//!   code, and no row refers to a row of another strip. Otherwise (Group 4
//!   strips, whose first rows are coded against an imaginary white row,
//!   and Group 3 strips without end-of-line codes) each strip is its own
//!   CCITT image, the page's band of rows, stacked on the page. A page
//!   whose strips don't account for its rows is refused, as is a tiled
//!   Group 3 page, which the `tiff` crate can't decode.
//! - **Everything else** (uncompressed, PackBits, LZW, Deflate, JPEG) is
//!   decoded by the `tiff` crate within the builder's limits, then follows
//!   the builder's rules: bilevel pages stay 1-bit (Flate), others become
//!   8-bit gray or RGB on white, scaled to the long-side cap, with Flate.
//!   Every plane of a planar page is read; alpha is composited as
//!   ExtraSamples says (associated alpha is premultiplied); CMYK and YCbCr
//!   (a JPEG-compressed colour page) become RGB.
//! - All pages together are held to the builder's document budget, a fax
//!   page's strips charged before they are copied (directories may share
//!   strips), and a cancelled import stops between pages.
//! - No tag but the image's own (size, resolution, orientation, coding)
//!   is read into the PDF: descriptions, software, dates and the like stay
//!   out.

use super::{
    Band, Budget, Ccitt, ImageError, MAX_ALLOC, MAX_DIMENSION, MAX_LONG_SIDE, Page, Stream,
};
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

/// Every page of the TIFF in `bytes`, in order, charged to `budget`, which
/// is checked for cancellation before each.
pub(super) fn pages(bytes: &[u8], budget: &mut Budget) -> Result<Vec<Page>, ImageError> {
    let mut decoder = Decoder::new(Cursor::new(bytes))
        .map_err(error)?
        .with_limits(limits());
    let mut pages = Vec::new();
    loop {
        if pages.len() == MAX_PAGES {
            return Err(ImageError::TooLarge);
        }
        budget.check()?;
        pages.push(page(&mut decoder, bytes, budget)?);
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

/// The current directory as a page, charged to `budget`.
fn page(decoder: &mut TiffDecoder, bytes: &[u8], budget: &mut Budget) -> Result<Page, ImageError> {
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
        3 | 4 => match fax(decoder, bytes, width, height, compression, budget)? {
            Some(page) => page,
            None => budget.keep(decoded(decoder, width, height)?)?,
        },
        _ => budget.keep(decoded(decoder, width, height)?)?,
    };
    // A page scaled down to the long-side cap keeps its physical size: its
    // resolution scales with its pixels (a 600 dpi Letter scan is stored
    // 3477 by 4500 pixels at about 409 dpi, still 8.5 by 11 inches).
    let resolution = resolution.map(|(x, y)| {
        (
            x * f64::from(page.width) / f64::from(width),
            y * f64::from(page.height) / f64::from(height),
        )
    });
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

/// A CCITT page, passed through: its strips joined, or each its own band
/// when they can't be; their bytes charged to `budget` before they are
/// copied. `None` when the `tiff` crate must decode it instead (tiles,
/// Group 4 uncompressed mode).
fn fax(
    decoder: &mut TiffDecoder,
    bytes: &[u8],
    width: u32,
    height: u32,
    compression: u16,
    budget: &mut Budget,
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
    let mut ranges = Vec::with_capacity(offsets.len());
    for (&offset, &count) in offsets.iter().zip(&counts) {
        let start = usize::try_from(offset).map_err(|_| ImageError::Unreadable)?;
        let end = start
            .checked_add(usize::try_from(count).map_err(|_| ImageError::Unreadable)?)
            .filter(|&end| end <= bytes.len())
            .ok_or(ImageError::Unreadable)?;
        ranges.push(start..end);
    }
    // Directories may share strips: a small file can name gigabytes.
    let total: usize = ranges.iter().map(|range| range.len()).sum();
    budget.charge(total as u64)?;
    let mut data = Vec::with_capacity(total);
    for range in &ranges {
        let strip = &bytes[range.clone()];
        if reversed {
            data.extend(strip.iter().map(|byte| byte.reverse_bits()));
        } else {
            data.extend_from_slice(strip);
        }
    }
    let mut strips = Vec::with_capacity(ranges.len());
    let mut at = 0;
    for range in &ranges {
        strips.push(&data[at..at + range.len()]);
        at += range.len();
    }
    // TIFF's T4Options bit 2 aligns each end-of-line code's end on a byte;
    // PDF's EncodedByteAlign would align the row data itself, skipping into
    // the next end-of-line code. Rows found by their end-of-line codes need
    // no alignment (PDFium loses rows with it); only rows without one do.
    let every_strip_has_eol = k >= 0 && strips.iter().all(|strip| starts_a_fresh_row(strip, false));
    let ccitt = Ccitt {
        k,
        // A min-is-black page's coded white runs show black.
        black_is_1: photometric == 1,
        byte_align: byte_align && !every_strip_has_eol,
    };
    let joinable = strips.len() == 1
        || (k >= 0
            && strips[1..]
                .iter()
                .all(|strip| starts_a_fresh_row(strip, k > 0)));
    let bands = if joinable {
        Vec::new()
    } else {
        // Each strip is a CCITT stream on its own, coded against a white
        // row above it: one image per strip, stacked (decision 12). A
        // layout whose strips don't account for the page's rows is broken.
        let rows_per_strip = tag::<u32>(decoder, Tag::RowsPerStrip)
            .filter(|&rows| rows > 0)
            .ok_or(ImageError::Unreadable)?;
        let count = strips.len() as u64;
        if u64::from(rows_per_strip) * count < u64::from(height)
            || u64::from(rows_per_strip) * (count - 1) >= u64::from(height)
        {
            return Err(ImageError::Unreadable);
        }
        strips
            .iter()
            .enumerate()
            .map(|(index, strip)| Band {
                rows: rows_per_strip.min(height - rows_per_strip * index as u32),
                bytes: strip.len(),
            })
            .collect()
    };
    Ok(Some(Page {
        width,
        height,
        gray: true,
        bits: 1,
        stream: Stream::Ccitt(ccitt),
        data,
        orientation: 1,
        resolution: None,
        bands,
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

/// The tests' twin of a CCITT page: its `strips` decoded one by one
/// (`hayro-ccitt`, a decoder independent of PDFium): rows packed
/// most significant bit first, padded to a byte, 1 meaning a coded white
/// pixel. Each strip holds `rows_per_strip` rows, the last what remains;
/// each may or may not open its rows with end-of-line codes.
#[cfg(test)]
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
#[cfg(test)]
struct Rows {
    width: u32,
    row_bytes: usize,
    packed: Vec<u8>,
}

#[cfg(test)]
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

/// The page decoded by the `tiff` crate, within the builder's limits: every
/// plane of a planar page, its alpha (ExtraSamples) composited on white,
/// CMYK and YCbCr turned to RGB, 16-bit samples to 8.
fn decoded(decoder: &mut TiffDecoder, width: u32, height: u32) -> Result<Page, ImageError> {
    let color = decoder.colortype().map_err(error)?;
    let (model, colors, bits) = match color {
        ColorType::Gray(bits) => (Model::Gray, 1, bits),
        // Gray with extra samples, which the decoder doesn't name.
        ColorType::Multiband { bit_depth, .. } => (Model::Gray, 1, bit_depth),
        ColorType::RGB(bits) | ColorType::RGBA(bits) => (Model::Rgb, 3, bits),
        ColorType::CMYK(bits) | ColorType::CMYKA(bits) => (Model::Cmyk, 4, bits),
        ColorType::YCbCr(bits) => (Model::YCbCr(luma_coefficients(decoder)), 3, bits),
        _ => return Err(ImageError::Unreadable),
    };
    let samples = tag::<u16>(decoder, Tag::SamplesPerPixel).unwrap_or(1);
    if samples < colors {
        return Err(ImageError::Unreadable);
    }
    let size =
        u64::from(width) * u64::from(height) * u64::from(samples) * u64::from(bits).div_ceil(8);
    if size > MAX_ALLOC {
        return Err(ImageError::TooLarge);
    }
    // The `tiff` crate gives a white-is-zero page as black-is-zero.
    if bits == 1 && samples == 1 {
        return bilevel(decoder.read_image().map_err(error)?, width, height);
    }
    if bits != 8 && bits != 16 {
        return Err(ImageError::Unreadable);
    }
    let alpha = alpha(decoder, samples > colors);
    // Every plane of a planar page, not only the first, which `read_image`
    // would give.
    let mut result = DecodingResult::U8(Vec::new());
    let layout = decoder.read_image_to_buffer(&mut result).map_err(error)?;
    let values = match result {
        DecodingResult::U8(values) => Values::U8(values),
        DecodingResult::U16(values) => Values::U16(values),
        _ => return Err(ImageError::Unreadable),
    };
    let sample_bytes = if bits == 16 { 2 } else { 1 };
    if values.len() * sample_bytes < layout.complete_len {
        // The decoder read the first plane only: all of them exceed its
        // limits.
        return Err(ImageError::TooLarge);
    }
    let pixels = width as usize * height as usize;
    let used = usize::from(colors) + usize::from(alpha != Alpha::None);
    let (plane, stride) = if layout.planes > 1 {
        let plane = layout.plane_stride.map_or(0, |stride| stride.get()) / sample_bytes;
        if layout.planes < used || plane < pixels {
            return Err(ImageError::Unreadable);
        }
        (plane, 1)
    } else {
        // The samples the decoder gives per pixel: an extra sample that
        // isn't alpha may or may not be among them.
        let stride = values.len() / pixels;
        if stride < used {
            return Err(ImageError::Unreadable);
        }
        (1, stride)
    };
    // The common case, 8-bit gray or RGB without alpha as the decoder lays
    // it out interleaved, is the image as it is.
    if layout.planes == 1
        && stride == usize::from(colors)
        && alpha == Alpha::None
        && matches!(model, Model::Gray | Model::Rgb)
        && let Values::U8(mut samples) = values
    {
        samples.truncate(pixels * stride);
        let image = if let Model::Gray = model {
            image::GrayImage::from_raw(width, height, samples).map(image::DynamicImage::ImageLuma8)
        } else {
            image::RgbImage::from_raw(width, height, samples).map(image::DynamicImage::ImageRgb8)
        };
        return super::encode(image.ok_or(ImageError::Unreadable)?, 1, false);
    }
    // Sample `index` of `pixel`: planes one after another, or interleaved.
    let sample = |pixel: usize, index: usize| values.get(index * plane + pixel * stride);
    let opacity = |pixel| {
        if alpha == Alpha::None {
            255
        } else {
            sample(pixel, usize::from(colors))
        }
    };
    let on_white = |value: u8, opacity: u8| match alpha {
        Alpha::None => value,
        // Premultiplied: the colour is already scaled by its opacity.
        Alpha::Associated => value.saturating_add(255 - opacity),
        Alpha::Unassociated => {
            let (value, opacity) = (u32::from(value), u32::from(opacity));
            ((value * opacity + 255 * (255 - opacity) + 127) / 255) as u8
        }
    };
    let image = if let Model::Gray = model {
        let luma = (0..pixels)
            .map(|pixel| on_white(sample(pixel, 0), opacity(pixel)))
            .collect();
        image::DynamicImage::ImageLuma8(
            image::GrayImage::from_raw(width, height, luma).ok_or(ImageError::Unreadable)?,
        )
    } else {
        let mut rgb = Vec::with_capacity(pixels * 3);
        for pixel in 0..pixels {
            let channels = [0, 1, 2].map(|channel| sample(pixel, channel));
            let color = match model {
                // TIFF stores CMYK uninverted.
                Model::Cmyk => {
                    let k = 255 - u32::from(sample(pixel, 3));
                    channels.map(|value| ((255 - u32::from(value)) * k / 255) as u8)
                }
                Model::YCbCr(coefficients) => ycbcr_to_rgb(channels, coefficients),
                Model::Gray | Model::Rgb => channels,
            };
            let opacity = opacity(pixel);
            rgb.extend(color.map(|value| on_white(value, opacity)));
        }
        image::DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(width, height, rgb).ok_or(ImageError::Unreadable)?,
        )
    };
    super::encode(image, 1, false)
}

/// How a page's colour samples read.
#[derive(Clone, Copy)]
enum Model {
    Gray,
    Rgb,
    Cmyk,
    /// Luma and chroma, with the luma's shares of red, green and blue.
    YCbCr([f64; 3]),
}

/// A page's alpha: its first extra sample, when ExtraSamples says it is one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Alpha {
    None,
    /// Associated: the colour samples are premultiplied by it.
    Associated,
    Unassociated,
}

/// The alpha of a page with an `extra` sample, from its ExtraSamples. An
/// extra sample that no ExtraSamples tag describes is read as unassociated
/// alpha, as the decoder reads a fourth RGB sample.
fn alpha(decoder: &mut TiffDecoder, extra: bool) -> Alpha {
    if !extra {
        return Alpha::None;
    }
    match decoder
        .find_tag_unsigned_vec::<u16>(Tag::ExtraSamples)
        .ok()
        .flatten()
        .as_deref()
    {
        Some([1, ..]) => Alpha::Associated,
        Some([2, ..]) | None => Alpha::Unassociated,
        Some(_) => Alpha::None,
    }
}

/// The YCbCrCoefficients tag, or the CCIR 601 shares TIFF defaults to.
fn luma_coefficients(decoder: &mut TiffDecoder) -> [f64; 3] {
    let default = [0.299, 0.587, 0.114];
    let Some(Value::List(values)) = decoder.find_tag(Tag::Unknown(529)).ok().flatten() else {
        return default;
    };
    let shares: Vec<f64> = values
        .iter()
        .filter_map(|value| match *value {
            Value::Rational(numerator, denominator) if denominator > 0 => {
                Some(f64::from(numerator) / f64::from(denominator))
            }
            _ => None,
        })
        .collect();
    match shares[..] {
        [red, green, blue] if red > 0. && green > 0. && blue > 0. => [red, green, blue],
        _ => default,
    }
}

/// A full-range YCbCr sample as RGB: TIFF's ReferenceBlackWhite default,
/// and what a JPEG-compressed page holds.
fn ycbcr_to_rgb([y, cb, cr]: [u8; 3], [red, green, blue]: [f64; 3]) -> [u8; 3] {
    let (y, cb, cr) = (f64::from(y), f64::from(cb) - 128., f64::from(cr) - 128.);
    let r = y + (2. - 2. * red) * cr;
    let b = y + (2. - 2. * blue) * cb;
    let g = (y - red * r - blue * b) / green;
    [r, g, b].map(|value| value.round().clamp(0., 255.) as u8)
}

/// Decoded samples, 8 or 16 bits each.
enum Values {
    U8(Vec<u8>),
    U16(Vec<u16>),
}

impl Values {
    fn len(&self) -> usize {
        match self {
            Self::U8(values) => values.len(),
            Self::U16(values) => values.len(),
        }
    }

    /// The sample at `index`, as 8 bits.
    fn get(&self, index: usize) -> u8 {
        match self {
            Self::U8(values) => values[index],
            Self::U16(values) => ((u32::from(values[index]) * 255 + 32_767) / 65_535) as u8,
        }
    }
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
        bands: Vec::new(),
    })
}

#[cfg(test)]
mod tests;
