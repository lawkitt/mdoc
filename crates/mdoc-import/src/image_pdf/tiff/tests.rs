//! TIFF pages at the builder's seam, on the synthetic fixtures in
//! `tests/fixtures/tiff` (fictional text; its README lists every tag).

use super::super::{DEFAULT_DPI, ImageError, build};
use super::starts_a_fresh_row;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/tiff")
        .join(name)
}

fn read(name: &str) -> Vec<u8> {
    std::fs::read(fixture(name)).unwrap()
}

/// One page of a built PDF: its size in points, and each of its images'
/// dictionary and stream, top band first (`image` and `data`: the first).
#[derive(Debug)]
struct Page {
    size: (f64, f64),
    image: String,
    data: Vec<u8>,
    images: Vec<(String, Vec<u8>)>,
}

/// Where `needle` occurs in `haystack`, in order.
fn positions(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    haystack
        .windows(needle.len())
        .enumerate()
        .filter(|(_, window)| *window == needle)
        .map(|(at, _)| at)
        .collect()
}

/// Every page of a PDF the builder wrote, in order. Searched as bytes: the
/// streams between the objects are binary.
fn pages(pdf: &[u8]) -> Vec<Page> {
    let boxes = positions(pdf, b"/MediaBox [0 0 ");
    let images: Vec<(usize, String, Vec<u8>)> = positions(pdf, b"/Subtype /Image")
        .into_iter()
        .map(|at| {
            let head_end = at + positions(&pdf[at..], b"\nstream\n")[0];
            let image = String::from_utf8_lossy(&pdf[at..head_end]).into_owned();
            let length_at = image.find("/Length ").unwrap() + 8;
            let length: usize = image[length_at..]
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .unwrap()
                .parse()
                .unwrap();
            let start = head_end + 8;
            (at, image, pdf[start..start + length].to_vec())
        })
        .collect();
    // A page's images follow its page object, before the next page's.
    boxes
        .iter()
        .enumerate()
        .map(|(index, &at)| {
            let rest = &pdf[at + 15..];
            let end = rest.iter().position(|&byte| byte == b']').unwrap();
            let numbers: Vec<f64> = String::from_utf8_lossy(&rest[..end])
                .split_whitespace()
                .map(|number| number.parse().unwrap())
                .collect();
            let next = boxes.get(index + 1).copied().unwrap_or(usize::MAX);
            let images: Vec<(String, Vec<u8>)> = images
                .iter()
                .filter(|(image_at, _, _)| (at..next).contains(image_at))
                .map(|(_, image, data)| (image.clone(), data.clone()))
                .collect();
            Page {
                size: (numbers[0], numbers[1]),
                image: images[0].0.clone(),
                data: images[0].1.clone(),
                images,
            }
        })
        .collect()
}

fn approx(actual: (f64, f64), expected: (f64, f64)) -> bool {
    (actual.0 - expected.0).abs() < 0.01 && (actual.1 - expected.1).abs() < 0.01
}

/// Every fixture gives every page, each sized by its own resolution, the
/// CCITT pages passed through with their parameters (the multi-strip G4
/// pages as one image per strip), the LZW pages decoded and stored with
/// Flate.
#[test]
fn every_page_of_every_fixture_is_imported() {
    // Fax fine and fax normal are the same sheet: 1728 px at 204 dpi
    // across, 2200 rows at 196 dpi or 1100 at 98 dpi down.
    let fax_sheet = (1728. * 72. / 204., 2200. * 72. / 196.);
    let letter = (612., 792.);
    for (name, count, size, filter, images) in [
        ("fax-g3-1d.tif", 3, fax_sheet, "/CCITTFaxDecode", 1),
        (
            "fax-g3-2d-normal-lsb.tif",
            2,
            fax_sheet,
            "/CCITTFaxDecode",
            1,
        ),
        ("fax-g4.tif", 2, letter, "/CCITTFaxDecode", 1),
        ("fax-g4-strips.tif", 2, letter, "/CCITTFaxDecode", 5),
        ("scan-lzw-gray.tif", 2, letter, "/FlateDecode", 1),
        ("scan-lzw-rgb.tif", 1, letter, "/FlateDecode", 1),
    ] {
        let built = build(&[&read(name)], DEFAULT_DPI).unwrap();
        assert_eq!(built.pages, count, "{name}");
        assert_eq!(built.frames_left_out, 0, "{name}");
        let pages = pages(&built.pdf);
        assert_eq!(pages.len(), count as usize, "{name}");
        for page in &pages {
            assert!(approx(page.size, size), "{name}: {:?}", page.size);
            assert_eq!(page.images.len(), images, "{name}");
            for (image, _) in &page.images {
                assert!(image.contains(filter), "{name}: {image}");
            }
        }
    }
}

/// The CCITT pages carry the decoding parameters their tags call for. A
/// Group 3 fax whose rows open with end-of-line codes is read by them, not
/// by byte alignment: TIFF's T4Options bit 2 aligns the end of each code,
/// and PDF's EncodedByteAlign would skip into the next one (PDFium loses
/// rows with it; the qualification checks every row).
#[test]
fn fax_pages_pass_through_with_their_parameters() {
    for (name, parameters) in [
        (
            "fax-g3-1d.tif",
            "/K 0 /Columns 1728 /Rows 2200 /BlackIs1 false /EncodedByteAlign false",
        ),
        (
            "fax-g3-2d-normal-lsb.tif",
            "/K 1100 /Columns 1728 /Rows 1100 /BlackIs1 true /EncodedByteAlign false",
        ),
        (
            "fax-g4.tif",
            "/K -1 /Columns 2550 /Rows 3300 /BlackIs1 false /EncodedByteAlign false",
        ),
    ] {
        let built = build(&[&read(name)], DEFAULT_DPI).unwrap();
        for page in pages(&built.pdf) {
            assert!(page.image.contains(parameters), "{name}: {}", page.image);
            assert!(
                page.image
                    .contains("/ColorSpace /DeviceGray /BitsPerComponent 1"),
                "{name}: {}",
                page.image
            );
        }
    }
}

/// The raw strips of the TIFF's `page`-th directory, as the file holds
/// them.
fn raw_strips(bytes: &[u8], page: usize) -> Vec<Vec<u8>> {
    use tiff::{decoder::Decoder, tags::Tag};
    let mut decoder = Decoder::new(std::io::Cursor::new(bytes)).unwrap();
    for _ in 0..page {
        decoder.next_image().unwrap();
    }
    let offsets: Vec<u64> = decoder.get_tag_u64_vec(Tag::StripOffsets).unwrap();
    let counts: Vec<u64> = decoder.get_tag_u64_vec(Tag::StripByteCounts).unwrap();
    offsets
        .iter()
        .zip(&counts)
        .map(|(&offset, &count)| bytes[offset as usize..(offset + count) as usize].to_vec())
        .collect()
}

/// A Group 3 2D page in four strips, least significant bit first: the
/// stream is the four strips joined, each byte's bits reversed, and every
/// strip opens with an end-of-line code and a one-dimensional row.
#[test]
fn several_group_3_strips_are_joined_bit_reversed() {
    let bytes = read("fax-g3-2d-normal-lsb.tif");
    let built = build(&[&bytes], DEFAULT_DPI).unwrap();
    for (index, page) in pages(&built.pdf).iter().enumerate() {
        let strips = raw_strips(&bytes, index);
        assert_eq!(strips.len(), 4);
        let reversed: Vec<Vec<u8>> = strips
            .iter()
            .map(|strip| strip.iter().map(|byte| byte.reverse_bits()).collect())
            .collect();
        assert_eq!(page.data, reversed.concat());
        for strip in &reversed[1..] {
            assert!(starts_a_fresh_row(strip, true));
        }
        // As the file holds them, unreversed, they don't.
        assert!(!starts_a_fresh_row(&strips[1], true));
    }
}

/// A little-endian, one-page TIFF: `entries` (tag, type, value), each one
/// value long, then `arrays` (tag, type, values) of 32-bit values stored
/// after the directory, then `data`.
fn tiff_file(entries: &[(u16, u16, u32)], arrays: &[(u16, Vec<u32>)], data: &[u8]) -> Vec<u8> {
    let count = entries.len() + arrays.len();
    let directory_end = 8 + 2 + 12 * count + 4;
    let arrays_size: usize = arrays.iter().map(|(_, values)| 4 * values.len()).sum();
    let data_at = (directory_end + arrays_size) as u32;
    let mut all: Vec<(u16, u16, u32, u32)> = entries
        .iter()
        .map(|&(tag, kind, value)| (tag, kind, 1, value))
        .collect();
    let mut at = directory_end as u32;
    for (tag, values) in arrays {
        all.push((*tag, 4, values.len() as u32, at));
        at += 4 * values.len() as u32;
    }
    all.sort_by_key(|entry| entry.0);
    let mut tiff = b"II*\x00".to_vec();
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&(count as u16).to_le_bytes());
    for (tag, kind, count, value) in all {
        tiff.extend_from_slice(&tag.to_le_bytes());
        tiff.extend_from_slice(&kind.to_le_bytes());
        tiff.extend_from_slice(&count.to_le_bytes());
        tiff.extend_from_slice(&value.to_le_bytes());
    }
    tiff.extend_from_slice(&0u32.to_le_bytes());
    for (_, values) in arrays {
        for value in values {
            // Offsets are relative to the data, which follows the arrays.
            tiff.extend_from_slice(&value.to_le_bytes());
        }
    }
    assert_eq!(tiff.len() as u32, data_at);
    tiff.extend_from_slice(data);
    tiff
}

/// A Group 3 page whose strips carry no end-of-line codes can't be joined:
/// each strip is its own CCITT image, stacked (decision 12), byte-aligned
/// as its rows are. Here an 8-pixel-wide page of four rows in two strips,
/// coded by hand: a white row (white run 8, `10011`) and a black row (white
/// run 0 `00110101`, black run 8 `000101`), each row starting on a byte.
#[test]
fn group_3_strips_that_cant_be_joined_are_stacked_as_their_own_images() {
    let strip = [0x98, 0x35, 0x14];
    let header = 8 + 2 + 12 * 9 + 4 + 2 * 4 * 2;
    let tiff = tiff_file(
        &[
            (256, 3, 8),
            (257, 3, 4),
            (258, 3, 1),
            (259, 3, 3),
            (262, 3, 0),
            (278, 3, 2),
            (292, 4, 4),
        ],
        &[(273, vec![header, header + 3]), (279, vec![3, 3])],
        &[strip, strip].concat(),
    );
    let built = build(&[&tiff], DEFAULT_DPI).unwrap();
    let page = &pages(&built.pdf)[0];
    assert_eq!(page.images.len(), 2);
    for (image, data) in &page.images {
        assert!(
            image.contains("/Height 2 ")
                && image.contains("/K 0 /Columns 8 /Rows 2 /BlackIs1 false /EncodedByteAlign true"),
            "{image}"
        );
        assert_eq!(data, &strip);
        // Each decodes on its own: white, then black (1 is white).
        let rows = super::decode_strips(std::slice::from_ref(data), 8, 2, 2, 0, true).unwrap();
        assert_eq!(rows, [0xff, 0x00]);
    }
}

/// The joined Group 3 2D stream decodes to the same page as its strips
/// decoded one by one (303 rows each, the last 191): joining them is sound.
#[test]
fn joined_group_3_strips_decode_like_the_strips_one_by_one() {
    let bytes = read("fax-g3-2d-normal-lsb.tif");
    for page in 0..2 {
        let strips: Vec<Vec<u8>> = raw_strips(&bytes, page)
            .into_iter()
            .map(|strip| strip.iter().map(|byte| byte.reverse_bits()).collect())
            .collect();
        let one_by_one = super::decode_strips(&strips, 1728, 1100, 303, 1100, false).unwrap();
        let joined =
            super::decode_strips(&[strips.concat()], 1728, 1100, 1100, 1100, false).unwrap();
        assert_eq!(one_by_one, joined, "page {}", page + 1);
        let black = one_by_one
            .iter()
            .map(|byte| byte.count_zeros())
            .sum::<u32>();
        assert!(black > 10_000, "page {}: {black}", page + 1);
    }
}

/// Group 4 strips can't be joined: the page in five strips is five CCITT
/// images, stacked (decision 12), each exactly its strip, and together they
/// decode bit for bit to the page the single-strip file decodes to.
#[test]
fn group_4_pages_in_strips_are_stacked_as_their_own_images() {
    use tiff::decoder::{Decoder, DecodingResult};
    let single = read("fax-g4.tif");
    let mut decoder = Decoder::new(std::io::Cursor::new(&single[..])).unwrap();
    let built = build(&[&read("fax-g4-strips.tif")], DEFAULT_DPI).unwrap();
    for (index, page) in pages(&built.pdf).iter().enumerate() {
        if index > 0 {
            decoder.next_image().unwrap();
        }
        let DecodingResult::U8(packed) = decoder.read_image().unwrap() else {
            panic!("bilevel pages decode to bytes");
        };
        // The decoder gives the min-is-white page as black-is-zero: 1 is
        // white, as the CCITT decode gives coded white.
        let strips = raw_strips(&read("fax-g4-strips.tif"), index);
        assert_eq!(page.images.len(), strips.len());
        let mut rows = Vec::new();
        for ((image, data), strip) in page.images.iter().zip(&strips) {
            assert!(image.contains("/K -1 /Columns 2550 "), "{image}");
            assert_eq!(data, strip);
        }
        for (band, (image, data)) in page.images.iter().enumerate() {
            let height: u32 = image[image.find("/Height ").unwrap() + 8..]
                .split(' ')
                .next()
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(height, if band < 4 { 800 } else { 100 });
            rows.extend(
                super::decode_strips(std::slice::from_ref(data), 2550, height, height, -1, false)
                    .unwrap(),
            );
        }
        assert_eq!(rows.len(), 2550usize.div_ceil(8) * 3300);
        assert_eq!(rows, packed[..rows.len()]);
        // Text, not a blank or black page.
        let black = rows.iter().map(|byte| byte.count_zeros()).sum::<u32>();
        assert!((1_000..2_000_000).contains(&black), "{black} black pixels");
    }
}

/// A TIFF claiming more pixels than the limits allow is refused before
/// anything is decoded, whatever its coding.
#[test]
fn limits_refuse_an_oversized_tiff() {
    for compression in [1u16, 4] {
        // A little-endian TIFF: 40,000 by 40,000, one tiny strip.
        let mut tiff = b"II*\x00".to_vec();
        tiff.extend_from_slice(&8u32.to_le_bytes());
        let entries: [(u16, u16, u32); 9] = [
            (256, 4, 40_000),
            (257, 4, 40_000),
            (258, 3, if compression == 1 { 8 } else { 1 }),
            (259, 3, u32::from(compression)),
            (262, 3, 0),
            (273, 4, 200),
            (277, 3, 1),
            (278, 4, 40_000),
            (279, 4, 4),
        ];
        tiff.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        for (tag, kind, value) in entries {
            tiff.extend_from_slice(&tag.to_le_bytes());
            tiff.extend_from_slice(&kind.to_le_bytes());
            tiff.extend_from_slice(&1u32.to_le_bytes());
            tiff.extend_from_slice(&value.to_le_bytes());
        }
        tiff.extend_from_slice(&0u32.to_le_bytes());
        tiff.resize(204, 0);
        assert!(
            matches!(build(&[&tiff], DEFAULT_DPI), Err(ImageError::TooLarge)),
            "compression {compression}"
        );
    }
}

#[test]
fn damaged_tiffs_are_unreadable() {
    let mut truncated = read("scan-lzw-gray.tif");
    truncated.truncate(400);
    for bytes in [b"II*\x00\xff\xff\xff\xff".to_vec(), truncated] {
        assert!(matches!(
            build(&[&bytes], DEFAULT_DPI),
            Err(ImageError::Unreadable)
        ));
    }
}

/// Only the image reaches the PDF: no TIFF tag text (software, dates,
/// descriptions) does.
#[test]
fn no_tag_text_reaches_the_pdf() {
    for name in ["fax-g3-1d.tif", "scan-lzw-rgb.tif"] {
        let built = build(&[&read(name)], DEFAULT_DPI).unwrap();
        for leaked in [b"ImageMagick".as_slice(), b"2026:", b"Pillow"] {
            assert!(
                !built
                    .pdf
                    .windows(leaked.len())
                    .any(|window| window == leaked),
                "{name}: {}",
                String::from_utf8_lossy(leaked)
            );
        }
    }
}

/// At the import seam: each fixture imports as a scan whose every page
/// needs OCR, and a TIFF is admitted by extension and content.
#[test]
fn every_fixture_imports_every_page_for_ocr() {
    use crate::import::{ImportError, prepare, supported_extension, supported_source};
    for (name, count) in [
        ("fax-g3-1d.tif", 3),
        ("fax-g3-2d-normal-lsb.tif", 2),
        ("fax-g4.tif", 2),
        ("fax-g4-strips.tif", 2),
        ("scan-lzw-gray.tif", 2),
        ("scan-lzw-rgb.tif", 1),
    ] {
        match prepare(&fixture(name), None, false) {
            Err(ImportError::NeedsOcr(_, pages, _, page_count)) => {
                assert_eq!(page_count, count, "{name}");
                assert_eq!(pages, (1..=count).collect::<Vec<_>>(), "{name}");
            }
            other => panic!("{name}: {other:?}"),
        }
        assert!(supported_source(&fixture(name)));
    }
    for name in ["a.tif", "a.TIFF"] {
        assert!(supported_extension(Path::new(name)), "{name}");
    }
}

/// One directory's entries: (tag, type 3 SHORT, 4 LONG or 5 RATIONAL as
/// numerator and denominator pairs, values).
type Entries = Vec<(u16, u16, Vec<u32>)>;

/// A little-endian TIFF: `data` right after the header, then a directory
/// for each of `pages`, in order. StripOffsets (273) count from the start
/// of `data`.
fn tiff_of(pages: &[Entries], data: &[u8]) -> Vec<u8> {
    let mut tiff = b"II*\x00".to_vec();
    // Where the offset of the next directory goes: the header's first.
    let mut link = tiff.len();
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(data);
    for entries in pages {
        let mut entries = entries.clone();
        entries.sort_by_key(|entry| entry.0);
        let mut fields = Vec::new();
        for (tag, kind, values) in &entries {
            let values: Vec<u32> = if *tag == 273 {
                values.iter().map(|value| value + 8).collect()
            } else {
                values.clone()
            };
            let bytes: Vec<u8> = if *kind == 3 {
                values
                    .iter()
                    .flat_map(|&value| (value as u16).to_le_bytes())
                    .collect()
            } else {
                values
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .collect()
            };
            let value = if bytes.len() <= 4 {
                let mut inline = bytes;
                inline.resize(4, 0);
                inline
            } else {
                if tiff.len() % 2 == 1 {
                    tiff.push(0);
                }
                let at = tiff.len() as u32;
                tiff.extend_from_slice(&bytes);
                at.to_le_bytes().to_vec()
            };
            let count = if *kind == 5 {
                values.len() / 2
            } else {
                values.len()
            };
            fields.push((*tag, *kind, count as u32, value));
        }
        if tiff.len() % 2 == 1 {
            tiff.push(0);
        }
        let at = tiff.len() as u32;
        tiff[link..link + 4].copy_from_slice(&at.to_le_bytes());
        tiff.extend_from_slice(&(fields.len() as u16).to_le_bytes());
        for (tag, kind, count, value) in fields {
            tiff.extend_from_slice(&tag.to_le_bytes());
            tiff.extend_from_slice(&kind.to_le_bytes());
            tiff.extend_from_slice(&count.to_le_bytes());
            tiff.extend_from_slice(&value);
        }
        link = tiff.len();
        tiff.extend_from_slice(&0u32.to_le_bytes());
    }
    tiff
}

/// An uncompressed page's entries: `bits` per sample, one strip per plane
/// at `strips` (offset in the data, length).
fn page_entries(
    (width, height): (u32, u32),
    photometric: u32,
    bits: &[u32],
    strips: &[(u32, u32)],
) -> Entries {
    vec![
        (256, 4, vec![width]),
        (257, 4, vec![height]),
        (258, 3, bits.to_vec()),
        (259, 3, vec![1]),
        (262, 3, vec![photometric]),
        (273, 4, strips.iter().map(|strip| strip.0).collect()),
        (277, 3, vec![bits.len() as u32]),
        (278, 4, vec![height]),
        (279, 4, strips.iter().map(|strip| strip.1).collect()),
    ]
}

/// `entries` with `tag` set to `values`.
fn with(mut entries: Entries, tag: u16, kind: u16, values: &[u32]) -> Entries {
    entries.retain(|entry| entry.0 != tag);
    entries.push((tag, kind, values.to_vec()));
    entries
}

/// Each page of `tiff` as the builder stores it, decoded: gray or not, and
/// its 8-bit samples.
fn stored_pixels(tiff: &[u8]) -> Vec<(bool, Vec<u8>)> {
    use std::io::Read;
    let mut budget = super::super::Budget::new(u64::MAX, &|| false);
    super::pages(tiff, &mut budget)
        .unwrap()
        .into_iter()
        .map(|page| {
            assert_eq!(page.stream, super::super::Stream::Flate);
            let mut samples = Vec::new();
            flate2::read::ZlibDecoder::new(&page.data[..])
                .read_to_end(&mut samples)
                .unwrap();
            (page.gray, samples)
        })
        .collect()
}

/// Whether every sample is within `by` of what was expected.
fn close(actual: &[u8], expected: &[u8], by: u8) -> bool {
    actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected)
            .all(|(actual, expected)| actual.abs_diff(*expected) <= by)
}

/// A colour page compressed as a JPEG (compression 7) comes as YCbCr, which
/// is turned to RGB, beside the file's other pages, and imports with them
/// at the import seam.
#[test]
fn a_jpeg_compressed_colour_page_imports_beside_other_pages() {
    let (red, blue) = ([200u8, 40, 30], [30u8, 60, 200]);
    let photo = image::RgbImage::from_fn(16, 16, |x, _| image::Rgb(if x < 8 { red } else { blue }));
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 95)
        .encode_image(&photo)
        .unwrap();
    let gray = [200u8; 64];
    let mut data = jpeg.clone();
    data.extend_from_slice(&gray);
    let tiff = tiff_of(
        &[
            with(
                with(
                    page_entries((16, 16), 6, &[8, 8, 8], &[(0, jpeg.len() as u32)]),
                    259,
                    3,
                    &[7],
                ),
                278,
                4,
                &[16],
            ),
            page_entries((8, 8), 1, &[8], &[(jpeg.len() as u32, 64)]),
        ],
        &data,
    );
    let pages = stored_pixels(&tiff);
    assert_eq!(pages.len(), 2);
    let (is_gray, rgb) = &pages[0];
    assert!(!is_gray);
    // The middle of each half, away from the JPEG's edge between them.
    for (x, expected) in [(3usize, red), (12, blue)] {
        let at = (8 * 16 + x) * 3;
        assert!(
            close(&rgb[at..at + 3], &expected, 12),
            "{x}: {:?}",
            &rgb[at..at + 3]
        );
    }
    assert_eq!(pages[1], (true, gray.to_vec()));
    // At the import seam: both pages, for OCR.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mixed.tif");
    std::fs::write(&path, &tiff).unwrap();
    match crate::import::prepare(&path, None, false) {
        Err(crate::import::ImportError::NeedsOcr(_, pages, _, page_count)) => {
            assert_eq!((pages, page_count), (vec![1, 2], 2));
        }
        other => panic!("{other:?}"),
    }
}

/// Alpha is read as ExtraSamples says: associated alpha's colour is already
/// premultiplied, unassociated alpha's isn't, and gray with alpha (which
/// the decoder gives as two bands) is read too. Each goes on white.
#[test]
fn alpha_goes_on_white_as_extra_samples_say() {
    // One pixel: a sample of 100 at about half opacity (128).
    for (bits, extra, pixel, expected) in [
        // Associated: 100 + (255 - 128).
        (
            &[8u32, 8, 8, 8][..],
            1,
            vec![100u8, 100, 100, 128],
            vec![227u8, 227, 227],
        ),
        // Unassociated: (100 * 128 + 255 * 127) / 255.
        (
            &[8, 8, 8, 8],
            2,
            vec![100, 100, 100, 128],
            vec![177, 177, 177],
        ),
        // Unspecified: not alpha, so the colour shows as it is.
        (
            &[8, 8, 8, 8],
            0,
            vec![100, 100, 100, 128],
            vec![100, 100, 100],
        ),
        (&[8, 8], 1, vec![100, 128], vec![227]),
        (&[8, 8], 2, vec![100, 128], vec![177]),
    ] {
        let photometric = if bits.len() == 2 { 1 } else { 2 };
        let entries = with(
            page_entries((1, 1), photometric, bits, &[(0, pixel.len() as u32)]),
            338,
            3,
            &[extra],
        );
        let pages = stored_pixels(&tiff_of(&[entries], &pixel));
        assert_eq!(pages[0].1, expected, "{bits:?} extra {extra}");
    }
}

/// A planar RGB page gives all three planes, and 16-bit CMYK becomes RGB,
/// both at the import seam too.
#[test]
fn planar_rgb_and_16_bit_cmyk_pages_are_read() {
    // Two pixels: red and blue, each plane its own strip.
    let planes = [255u8, 0, 0, 0, 0, 255];
    let planar = with(
        page_entries((2, 1), 2, &[8, 8, 8], &[(0, 2), (2, 2), (4, 2)]),
        284,
        3,
        &[2],
    );
    // Two pixels: cyan ink at full (red off), and black at half.
    let cmyk: Vec<u8> = [65_535u16, 0, 0, 0, 0, 0, 0, 32_768]
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    let cmyk_page = page_entries((2, 1), 5, &[16, 16, 16, 16], &[(6, 16)]);
    let mut data = planes.to_vec();
    data.extend_from_slice(&cmyk);
    let tiff = tiff_of(&[planar, cmyk_page], &data);
    let pages = stored_pixels(&tiff);
    assert_eq!(pages[0], (false, vec![255, 0, 0, 0, 0, 255]));
    assert_eq!(pages[1], (false, vec![0, 255, 255, 127, 127, 127]));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("layouts.tif");
    std::fs::write(&path, &tiff).unwrap();
    match crate::import::prepare(&path, None, false) {
        Err(crate::import::ImportError::NeedsOcr(_, _, _, page_count)) => {
            assert_eq!(page_count, 2);
        }
        other => panic!("{other:?}"),
    }
}

/// A file's pages together may hold no more than the document budget, their
/// bytes charged before they are copied: here 129 directories share one
/// 2 MiB Group 4 strip, a small file naming 258 MiB of pages. At the import
/// seam it is too large.
#[test]
fn pages_together_are_held_to_the_document_budget() {
    let strip = vec![0u8; 2 << 20];
    let page = page_entries((8, 8), 0, &[1], &[(0, strip.len() as u32)]);
    let page = with(page, 259, 3, &[4]);
    let pages = vec![page; 129];
    let tiff = tiff_of(&pages, &strip);
    assert!(tiff.len() < (3 << 20));
    let source = crate::import::Source::new("big.tif".into(), None, tiff.clone());
    let cancel = std::sync::atomic::AtomicBool::new(false);
    assert!(matches!(
        crate::import::import_source(source, None, false, &cancel),
        Err(crate::import::ImportError::Image(ImageError::TooLarge))
    ));
    // Within a budget for exactly two pages, the third is refused before
    // its strip is copied.
    let mut budget = super::super::Budget::new(2 * strip.len() as u64, &|| false);
    assert!(matches!(
        super::pages(&tiff_of(&pages[..2], &strip), &mut budget),
        Ok(pages) if pages.len() == 2
    ));
    let mut budget = super::super::Budget::new(2 * strip.len() as u64, &|| false);
    assert!(matches!(
        super::pages(&tiff_of(&pages[..3], &strip), &mut budget),
        Err(ImageError::TooLarge)
    ));
}

/// A white, 1-bit page `width` by `height` at `dpi`, uncompressed.
fn white_scan(width: u32, height: u32, dpi: u32) -> Vec<u8> {
    let data = vec![0u8; width.div_ceil(8) as usize * height as usize];
    let page = page_entries((width, height), 0, &[1], &[(0, data.len() as u32)]);
    let page = with(page, 282, 5, &[dpi, 1]);
    let page = with(page, 283, 5, &[dpi, 1]);
    tiff_of(&[with(page, 296, 3, &[2])], &data)
}

/// A page scaled down to the long-side cap keeps its physical size: a 600
/// dpi Letter scan (5100 by 6600) is stored 3477 by 4500 and stays Letter,
/// which OCR renders at 1275 by 1650 at its default 150 dpi and 2550 by
/// 3300 at 300, under the cap either way.
#[test]
fn a_scaled_down_scan_keeps_its_physical_size() {
    let scan = white_scan(5100, 6600, 600);
    let built = build(&[&scan], DEFAULT_DPI).unwrap();
    let page = &pages(&built.pdf)[0];
    assert!(approx(page.size, (612., 792.)), "{:?}", page.size);
    assert!(
        page.image.contains("/Width 3477 /Height 4500 "),
        "{}",
        page.image
    );
    for dpi in [150., 300.] {
        let raster = (page.size.0 / 72. * dpi, page.size.1 / 72. * dpi);
        assert!(approx(raster, (8.5 * dpi, 11. * dpi)), "{raster:?}");
        assert!(raster.1 <= f64::from(super::super::MAX_LONG_SIDE));
    }
}

/// A sheet OCR would render longer than the cap is made smaller: a
/// 20-inch strip at 300 dpi (1000 by 6000, stored 750 by 4500) keeps its
/// 20 inches for OCR at 150 dpi (3000 pixels), and is drawn 15 inches long
/// for OCR at 300 dpi, 4500 pixels, not 6000.
#[test]
fn no_sheet_renders_longer_than_the_cap_for_ocr() {
    let scan = white_scan(1000, 6000, 300);
    for (dpi, inches) in [(150u16, 20.), (300, 15.)] {
        let built = build(&[&scan], dpi).unwrap();
        let page = &pages(&built.pdf)[0];
        assert!(
            approx(page.size, (inches / 6. * 72., inches * 72.)),
            "{dpi}: {:?}",
            page.size
        );
        assert!(page.size.1 / 72. * f64::from(dpi) <= 4500.5);
    }
}

/// A cancelled import stops between pages: the builder asks before each
/// page, and stops at the first yes, without reading the rest. At the
/// import seam a cancelled build is a cancelled import.
#[test]
fn cancelling_stops_between_pages() {
    use std::cell::Cell;
    let asked = Cell::new(0);
    // Yes on the third question: the first is before the file, the second
    // before its first page.
    let cancelled = || {
        asked.set(asked.get() + 1);
        asked.get() == 3
    };
    let bytes = read("fax-g3-1d.tif");
    assert!(matches!(
        super::super::build_cancellable(&[&bytes], DEFAULT_DPI, &cancelled),
        Err(ImageError::Cancelled)
    ));
    assert_eq!(asked.get(), 3, "stopped after the first of three pages");
    let source = crate::import::Source::read(&fixture("fax-g3-1d.tif")).unwrap();
    let cancel = std::sync::atomic::AtomicBool::new(true);
    assert!(matches!(
        crate::import::import_source(source, None, false, &cancel),
        Err(crate::import::ImportError::Cancelled)
    ));
}

/// The qualification with OCR and PDFium, on the targets OCR supports.
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64")
))]
mod qualification {
    use super::{DEFAULT_DPI, fixture, read};

    /// The decoded twin of a bilevel page: 1 meaning white, rows byte-padded.
    type Twin = (u32, u32, Vec<u8>);

    /// `page` of a Group 4 (or any `tiff`-decodable) fixture, decoded.
    fn tiff_twin(name: &str, page: usize) -> Twin {
        use tiff::decoder::{Decoder, DecodingResult};
        let bytes = read(name);
        let mut decoder = Decoder::new(std::io::Cursor::new(&bytes[..])).unwrap();
        for _ in 0..page {
            decoder.next_image().unwrap();
        }
        let (width, height) = decoder.dimensions().unwrap();
        let DecodingResult::U8(packed) = decoder.read_image().unwrap() else {
            panic!("bilevel pages decode to bytes");
        };
        (width, height, packed)
    }

    /// `page` of a Group 3 fixture decoded strip by strip (`hayro-ccitt`, not
    /// PDFium), as shown: 1 meaning white.
    fn ccitt_twin(name: &str, page: usize) -> Twin {
        use tiff::{decoder::Decoder, tags::Tag};
        let bytes = read(name);
        let mut decoder = Decoder::new(std::io::Cursor::new(&bytes[..])).unwrap();
        for _ in 0..page {
            decoder.next_image().unwrap();
        }
        let (width, height) = decoder.dimensions().unwrap();
        let mut tag = |tag| decoder.find_tag_unsigned::<u32>(tag).unwrap();
        let options = tag(Tag::Unknown(292)).unwrap_or(0);
        let reversed = tag(Tag::FillOrder) == Some(2);
        let min_is_black = tag(Tag::PhotometricInterpretation) == Some(1);
        let rows_per_strip = tag(Tag::RowsPerStrip).unwrap_or(height);
        let strips: Vec<Vec<u8>> = super::raw_strips(&bytes, page)
            .into_iter()
            .map(|strip| {
                if reversed {
                    strip.iter().map(|byte| byte.reverse_bits()).collect()
                } else {
                    strip
                }
            })
            .collect();
        let k = if options & 1 != 0 { 1 } else { 0 };
        let mut bits = super::super::decode_strips(
            &strips,
            width,
            height,
            rows_per_strip,
            k,
            options & 4 != 0,
        )
        .unwrap();
        if min_is_black {
            bits.iter_mut().for_each(|byte| *byte = !*byte);
        }
        (width, height, bits)
    }

    /// The rows of `twin` that PDFium draws differently from it, rendering
    /// the built page at one pixel per fax pixel: the page's resolution is
    /// made square at its horizontal one, so a 204 × 98 dpi fax renders
    /// 1728 × 1100 at 204 dpi. Every pixel of every row is compared.
    fn rows_drawn_otherwise(
        renderer: &pdf_inspector::vision::PdfiumRenderer,
        name: &str,
        page: usize,
        twin: &Twin,
    ) -> usize {
        use pdf_inspector::vision::{RenderOptions, RenderPixelFormat};
        let mut built = crate::image_pdf::pages(
            crate::image_pdf::ImageFormat::Tiff,
            &read(name),
            &mut crate::image_pdf::Budget::new(u64::MAX, &|| false),
        )
        .unwrap()
        .remove(page);
        let dpi = built.resolution.unwrap().0;
        built.resolution = Some((dpi, dpi));
        let pdf = crate::image_pdf::pdf::write(&[built], DEFAULT_DPI);
        let rendered = renderer
            .render_pages(
                &pdf,
                &[1],
                None,
                &RenderOptions::new()
                    .dpi(dpi as f32)
                    .pixel_format(RenderPixelFormat::Gray8),
            )
            .unwrap()
            .remove(0);
        let (width, height, bits) = twin;
        assert_eq!(
            (rendered.width(), rendered.height()),
            (*width, *height),
            "{name}"
        );
        let (stride, luma) = (rendered.stride(), rendered.pixels());
        let row = width.div_ceil(8) as usize;
        (0..*height as usize)
            .filter(|&y| {
                (0..*width as usize).any(|x| {
                    let white = bits[y * row + x / 8] >> (7 - x % 8) & 1 == 1;
                    white != (luma[y * stride + x] >= 128)
                })
            })
            .count()
    }

    /// The qualification the item's Done-when asks for, with the pinned OCR
    /// runtime and models downloaded to throwaway storage: OCR reads every
    /// page of every fixture, English and Russian, in order; and PDFium, which
    /// OCR renders with, draws each passed-through CCITT page like its decoded
    /// twin.
    #[test]
    #[ignore = "downloads OCR runtimes/models; requires Apple Silicon macOS or Windows x64"]
    fn ocr_reads_every_page_and_pdfium_draws_fax_pages_like_their_twins() {
        use pdf_inspector::vision::PdfiumRenderer;
        let dir = tempfile::tempdir().unwrap();
        let installed = crate::ocr::install_in(dir.path()).unwrap();
        let notice = ["NOTICE OF TERMINATION", "УВЕДОМЛЕНИЕ"];
        for (name, pages) in [
            (
                "fax-g3-1d.tif",
                &["SUPPLY AGREEMENT", "ДОГОВОР ПОСТАВКИ", "SIGNATURES"][..],
            ),
            (
                "fax-g3-2d-normal-lsb.tif",
                &["SUPPLY AGREEMENT", "ДОГОВОР ПОСТАВКИ"][..],
            ),
            ("fax-g4.tif", &notice[..]),
            ("fax-g4-strips.tif", &notice[..]),
            ("scan-lzw-gray.tif", &notice[..]),
            ("scan-lzw-rgb.tif", &notice[..1]),
        ] {
            let imported = crate::import::prepare(&fixture(name), Some(&installed), false).unwrap();
            assert!(imported.is_image && imported.is_pdf, "{name}");
            let markdown = imported.markdown.to_uppercase();
            let mut from = 0;
            for heading in pages {
                let found = markdown[from..]
                    .find(heading)
                    .unwrap_or_else(|| panic!("{name}: {heading} after {from} in {markdown}"));
                from += found + heading.len();
            }
        }
        let renderer = PdfiumRenderer::load_from_path(&installed.pdfium).unwrap();
        let mut twins: Vec<(&str, usize, Twin)> = Vec::new();
        for page in 0..3 {
            twins.push(("fax-g3-1d.tif", page, ccitt_twin("fax-g3-1d.tif", page)));
        }
        for page in 0..2 {
            let name = "fax-g3-2d-normal-lsb.tif";
            twins.push((name, page, ccitt_twin(name, page)));
            for name in ["fax-g4.tif", "fax-g4-strips.tif"] {
                twins.push((name, page, tiff_twin(name, page)));
            }
        }
        for (name, page, twin) in &twins {
            let wrong = rows_drawn_otherwise(&renderer, name, *page, twin);
            println!("{name} page {}: {wrong} rows drawn otherwise", page + 1);
            assert_eq!(wrong, 0, "{name} page {}", page + 1);
        }
    }
}
