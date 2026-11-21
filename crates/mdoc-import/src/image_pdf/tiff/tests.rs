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

/// One page of a built PDF: its size in points, its image's dictionary and
/// its image's stream.
#[derive(Debug)]
struct Page {
    size: (f64, f64),
    image: String,
    data: Vec<u8>,
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
    let boxes = positions(pdf, b"/MediaBox [0 0 ").into_iter().map(|at| {
        let rest = &pdf[at + 15..];
        let end = rest.iter().position(|&byte| byte == b']').unwrap();
        let numbers: Vec<f64> = String::from_utf8_lossy(&rest[..end])
            .split_whitespace()
            .map(|number| number.parse().unwrap())
            .collect();
        (numbers[0], numbers[1])
    });
    let images = positions(pdf, b"/Subtype /Image").into_iter().map(|at| {
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
        (image, pdf[start..start + length].to_vec())
    });
    boxes
        .zip(images)
        .map(|(size, (image, data))| Page { size, image, data })
        .collect()
}

fn inflate(data: &[u8]) -> Vec<u8> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .read_to_end(&mut out)
        .unwrap();
    out
}

fn approx(actual: (f64, f64), expected: (f64, f64)) -> bool {
    (actual.0 - expected.0).abs() < 0.01 && (actual.1 - expected.1).abs() < 0.01
}

/// Every fixture gives every page, each sized by its own resolution, the
/// CCITT pages passed through with their parameters, the LZW pages
/// decoded and stored with Flate.
#[test]
fn every_page_of_every_fixture_is_imported() {
    // Fax fine and fax normal are the same sheet: 1728 px at 204 dpi
    // across, 2200 rows at 196 dpi or 1100 at 98 dpi down.
    let fax_sheet = (1728. * 72. / 204., 2200. * 72. / 196.);
    let letter = (612., 792.);
    for (name, count, size, filter) in [
        ("fax-g3-1d.tif", 3, fax_sheet, "/CCITTFaxDecode"),
        ("fax-g3-2d-normal-lsb.tif", 2, fax_sheet, "/CCITTFaxDecode"),
        ("fax-g4.tif", 2, letter, "/CCITTFaxDecode"),
        ("fax-g4-strips.tif", 2, letter, "/FlateDecode"),
        ("scan-lzw-gray.tif", 2, letter, "/FlateDecode"),
        ("scan-lzw-rgb.tif", 1, letter, "/FlateDecode"),
    ] {
        let built = build(&[&read(name)], DEFAULT_DPI).unwrap();
        assert_eq!(built.pages, count, "{name}");
        assert_eq!(built.frames_left_out, 0, "{name}");
        let pages = pages(&built.pdf);
        assert_eq!(pages.len(), count as usize, "{name}");
        for page in &pages {
            assert!(approx(page.size, size), "{name}: {:?}", page.size);
            assert!(page.image.contains(filter), "{name}: {}", page.image);
        }
    }
}

/// The CCITT pages carry the decoding parameters their tags call for.
#[test]
fn fax_pages_pass_through_with_their_parameters() {
    for (name, parameters) in [
        (
            "fax-g3-1d.tif",
            "/K 0 /Columns 1728 /Rows 2200 /BlackIs1 false /EncodedByteAlign true",
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

/// A Group 3 page whose later strip doesn't open with an end-of-line code
/// can't be joined, and nothing here decodes Group 3: it is refused, not
/// shown wrong.
#[test]
fn group_3_strips_that_cant_be_joined_are_refused() {
    let mut bytes = read("fax-g3-2d-normal-lsb.tif");
    use tiff::{decoder::Decoder, tags::Tag};
    let second = Decoder::new(std::io::Cursor::new(&bytes[..]))
        .unwrap()
        .get_tag_u64_vec(Tag::StripOffsets)
        .unwrap()[1] as usize;
    bytes[second] = 0xff;
    assert!(matches!(
        build(&[&bytes], DEFAULT_DPI),
        Err(ImageError::Unreadable)
    ));
}

/// Group 4 strips can't be joined: the page in five strips is decoded and
/// stored losslessly, bit for bit the page the single-strip file decodes
/// to (1 meaning white).
#[test]
fn group_4_pages_in_strips_are_decoded_losslessly() {
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
        // white, as the PDF stores it.
        let expected = packed;
        assert!(
            page.image
                .contains("/ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /FlateDecode"),
            "{}",
            page.image
        );
        let stored = inflate(&page.data);
        assert_eq!(stored.len(), 2550usize.div_ceil(8) * 3300);
        assert_eq!(stored, expected[..stored.len()]);
        // Text, not a blank or black page.
        let black = stored.iter().map(|byte| byte.count_zeros()).sum::<u32>();
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

/// The qualification with OCR and PDFium, on the targets OCR supports.
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64")
))]
mod qualification {
    use super::{DEFAULT_DPI, build, fixture, read};

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

    /// The share of `twin`'s pixels whose colour the rendering (gray, one byte
    /// a pixel, any size) disagrees with, sampling the rendering where each
    /// pixel's centre lands.
    fn disagreement(twin: &Twin, rendered: (u32, u32, usize, &[u8])) -> f64 {
        let (width, height, bits) = twin;
        let (rendered_width, rendered_height, stride, luma) = rendered;
        let row = width.div_ceil(8) as usize;
        let mut wrong = 0u64;
        for y in 0..*height {
            for x in 0..*width {
                let white = bits[y as usize * row + x as usize / 8] >> (7 - x % 8) & 1 == 1;
                let rx =
                    ((f64::from(x) + 0.5) * f64::from(rendered_width) / f64::from(*width)) as u32;
                let ry =
                    ((f64::from(y) + 0.5) * f64::from(rendered_height) / f64::from(*height)) as u32;
                let shown_white = luma[ry as usize * stride + rx as usize] >= 128;
                if white != shown_white {
                    wrong += 1;
                }
            }
        }
        wrong as f64 / (f64::from(*width) * f64::from(*height))
    }

    /// The qualification the item's Done-when asks for, with the pinned OCR
    /// runtime and models downloaded to throwaway storage: OCR reads every
    /// page of every fixture, English and Russian, in order; and PDFium, which
    /// OCR renders with, draws each passed-through CCITT page like its decoded
    /// twin.
    #[test]
    #[ignore = "downloads OCR runtimes/models; requires Apple Silicon macOS or Windows x64"]
    fn ocr_reads_every_page_and_pdfium_draws_fax_pages_like_their_twins() {
        use pdf_inspector::vision::{PdfiumRenderer, RenderOptions, RenderPixelFormat};
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
        for (name, page, twin) in [
            ("fax-g4.tif", 0, tiff_twin("fax-g4.tif", 0)),
            ("fax-g4.tif", 1, tiff_twin("fax-g4.tif", 1)),
        ] {
            let built = build(&[&read(name)], DEFAULT_DPI).unwrap();
            let rendered = renderer
                .render_pages(
                    &built.pdf,
                    &[page as u32 + 1],
                    None,
                    &RenderOptions::new()
                        .dpi(300.)
                        .pixel_format(RenderPixelFormat::Gray8),
                )
                .unwrap()
                .remove(0);
            let wrong = disagreement(
                &twin,
                (
                    rendered.width(),
                    rendered.height(),
                    rendered.stride(),
                    rendered.pixels(),
                ),
            );
            assert!(wrong < 0.002, "{name} page {}: {wrong}", page + 1);
        }
    }
}
