//! HEIC at the builder's and the import's seam. A stub stands in for the
//! operating system's decoder, so CI needs no codec; the platform tests
//! decode the real fixtures (`tests/fixtures/heic`, fictional page, see its
//! README).

use super::{MissingCodec, OsDecoded, OsDecoder, is_heic, with_decoder};
use crate::image_pdf::{DEFAULT_DPI, ImageError, ImageFormat, build};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/heic")
        .join(name)
}

fn read(name: &str) -> Vec<u8> {
    std::fs::read(fixture(name)).unwrap()
}

const FIXTURES: [&str; 3] = [
    "page-upright.heic",
    "page-orientation-6.heic",
    "page-gps.heic",
];

/// The page as a decoder would give it: white, with a red block in the
/// stored top-left corner, `width` by `height`.
fn page(width: u32, height: u32) -> image::RgbImage {
    image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb(if x < width / 8 && y < height / 8 {
            [220, 30, 30]
        } else {
            [255, 255, 255]
        })
    })
}

/// A decoder that gives `page` with `orientation`, whatever the bytes, and
/// checks it was handed a HEIC file.
struct Stub {
    width: u32,
    height: u32,
    orientation: u8,
}

impl OsDecoder for Stub {
    fn decode_heic(&self, bytes: &[u8]) -> Result<OsDecoded, ImageError> {
        assert!(is_heic(bytes));
        Ok(OsDecoded {
            image: image::DynamicImage::ImageRgb8(page(self.width, self.height)),
            orientation: self.orientation,
        })
    }
}

/// A decoder that fails as a PC without the extensions does.
struct Missing(MissingCodec);

impl OsDecoder for Missing {
    fn decode_heic(&self, _: &[u8]) -> Result<OsDecoded, ImageError> {
        Err(ImageError::HeicCodecMissing(self.0))
    }
}

fn text(pdf: &[u8]) -> String {
    String::from_utf8_lossy(pdf).into_owned()
}

#[test]
fn heic_is_known_by_content_and_extension() {
    for name in FIXTURES {
        assert_eq!(
            ImageFormat::from_bytes(&read(name)),
            Some(ImageFormat::Heic),
            "{name}"
        );
    }
    for extension in ["heic", "HEIF", "hif"] {
        assert_eq!(
            ImageFormat::from_extension(extension),
            Some(ImageFormat::Heic)
        );
    }
    // AVIF is HEIF too, but AV1-coded: not this decoder's.
    let mut avif = b"\0\0\0\x1cftypavif\0\0\0\0avifmif1miaf".to_vec();
    avif.resize(64, 0);
    assert_eq!(ImageFormat::from_bytes(&avif), None);
}

/// An upright photo becomes one page of its own size, stored as a JPEG;
/// at the import seam it is one scanned page for OCR.
#[test]
fn an_upright_heic_imports_as_one_photo_page() {
    let stub = Stub {
        width: 1200,
        height: 1600,
        orientation: 1,
    };
    with_decoder(stub, || {
        let built = build(&[&read("page-upright.heic")], 72).unwrap();
        let pdf = text(&built.pdf);
        assert!(pdf.contains("/MediaBox [0 0 1200 1600]"), "{pdf}");
        assert!(pdf.contains("/Width 1200 /Height 1600 /ColorSpace /DeviceRGB"));
        assert!(pdf.contains("/Filter /DCTDecode"));
        match crate::import::prepare(&fixture("page-upright.heic"), None, false) {
            Err(crate::import::ImportError::NeedsOcr(_, pages, _, count)) => {
                assert_eq!((pages, count), (vec![1], 1));
            }
            other => panic!("{other:?}"),
        }
    });
}

/// A decoder that gives the pixels as stored with orientation 6 (WIC):
/// the pixels are turned before they are encoded (decision 13), so the
/// JPEG itself is upright, 1200 × 1600, drawn by a plain matrix, and the
/// red block stored top-left shows top-right, as a quarter turn clockwise
/// puts it.
#[test]
fn an_oriented_heic_is_stored_as_an_upright_jpeg() {
    let stub = Stub {
        width: 1600,
        height: 1200,
        orientation: 6,
    };
    with_decoder(stub, || {
        let built = build(&[&read("page-orientation-6.heic")], 72).unwrap();
        let pdf = text(&built.pdf);
        assert!(pdf.contains("/MediaBox [0 0 1200 1600]"), "{pdf}");
        assert!(pdf.contains("/Width 1200 /Height 1600"), "{pdf}");
        assert!(pdf.contains("q 1200 0 0 1600 0 0 cm"), "{pdf}");
        let jpeg = jpeg_in(&built.pdf);
        let image = image::load_from_memory(jpeg).unwrap().into_rgb8();
        assert_eq!(image.dimensions(), (1200, 1600));
        let red = |pixel: &image::Rgb<u8>| pixel[0] > 170 && pixel[1] < 100 && pixel[2] < 100;
        assert!(red(image.get_pixel(1150, 50)), "top-right");
        assert!(!red(image.get_pixel(50, 50)), "not top-left");
    });
}

/// The JPEG stream of a built one-page PDF.
fn jpeg_in(pdf: &[u8]) -> &[u8] {
    let start = pdf.windows(2).position(|w| w == [0xff, 0xd8]).unwrap();
    let end = start
        + pdf[start..]
            .windows(2)
            .position(|w| w == [0xff, 0xd9])
            .unwrap()
        + 2;
    &pdf[start..end]
}

/// A decoder that gives a photo with an alpha plane: transparent black on
/// the left half, half-transparent black on the next quarter, opaque red
/// on the last.
struct Transparent;

impl OsDecoder for Transparent {
    fn decode_heic(&self, _: &[u8]) -> Result<OsDecoded, ImageError> {
        Ok(OsDecoded {
            image: image::DynamicImage::ImageRgba8(image::RgbaImage::from_fn(64, 64, |x, _| {
                image::Rgba(transparent_pixel(x))
            })),
            orientation: 1,
        })
    }
}

/// The pixel `transparent.heic` holds in column `x` (its README).
fn transparent_pixel(x: u32) -> [u8; 4] {
    match x {
        0..32 => [0, 0, 0, 0],
        32..48 => [0, 0, 0, 128],
        _ => [220, 30, 30, 255],
    }
}

/// How column `x` of the stored page shows, put on white.
fn on_white(x: u32) -> [u8; 3] {
    match x {
        0..32 => [255, 255, 255],
        32..48 => [127, 127, 127],
        _ => [220, 30, 30],
    }
}

/// Transparency goes on white: what lies under a transparent pixel never
/// shows.
#[test]
fn a_transparent_heic_goes_on_white() {
    with_decoder(Transparent, || {
        let built = build(&[&read("transparent.heic")], DEFAULT_DPI).unwrap();
        let image = image::load_from_memory(jpeg_in(&built.pdf))
            .unwrap()
            .into_rgb8();
        for x in [8, 40, 56] {
            let stored = image.get_pixel(x, 32).0;
            let expected = on_white(x);
            assert!(
                stored
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| a.abs_diff(b) <= 12),
                "{x}: {stored:?}"
            );
        }
    });
}

/// The size check every decoder asks with its stored size before it
/// decodes: 30,000 pixels a side at most, and 512 MiB as RGBA.
#[test]
fn sizes_over_the_limits_are_refused() {
    use super::fits;
    for (width, height) in [(1200, 1600), (30_000, 16), (16, 30_000), (11_000, 11_000)] {
        assert!(fits(width, height).is_ok(), "{width} x {height}");
    }
    for (width, height) in [(30_001, 16), (16, 40_000), (12_000, 12_000)] {
        assert!(
            matches!(fits(width, height), Err(ImageError::TooLarge)),
            "{width} x {height}"
        );
    }
}

/// The alpha plane is found by its auxiliary type: `transparent.heic`'s item
/// 2 is the primary's alpha; the same item typed as depth (as phones store
/// depth maps) is not, nor is anything in an opaque photo. Naming it
/// primary in a copy changes only the `pitm` box.
#[test]
fn the_alpha_plane_is_found_by_its_auxiliary_type() {
    use super::{alpha_item, with_primary};
    let bytes = read("transparent.heic");
    assert_eq!(alpha_item(&bytes), Some(2));
    let depth = bytes
        .windows(7)
        .position(|window| window == b"auxid:1")
        .map(|at| {
            let mut depth = bytes.clone();
            depth[at + 6] = b'2';
            depth
        })
        .unwrap();
    assert_eq!(alpha_item(&depth), None);
    for name in FIXTURES.into_iter().chain(["two-images.heic"]) {
        assert_eq!(alpha_item(&read(name)), None, "{name}");
    }
    let copy = with_primary(&bytes, 2).unwrap();
    let changed: Vec<usize> = (0..bytes.len())
        .filter(|&at| bytes[at] != copy[at])
        .collect();
    assert_eq!(changed.len(), 1);
    assert_eq!(copy[changed[0]], 2);
}

/// A HEIF of the generic brand `mif1` is HEIC when its primary item is
/// HEVC-coded, and imports through the decoder at the import seam; one
/// whose items are AV1 (AVIF) is not.
#[test]
fn a_generic_heif_is_known_by_its_primary_item() {
    let heif = generic_heif(*b"hvc1");
    assert!(is_heic(&heif));
    assert_eq!(ImageFormat::from_bytes(&heif), Some(ImageFormat::Heic));
    assert_eq!(ImageFormat::from_bytes(&generic_heif(*b"av01")), None);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photo.heif");
    std::fs::write(&path, &heif).unwrap();
    let stub = Stub {
        width: 120,
        height: 160,
        orientation: 1,
    };
    with_decoder(stub, || match crate::import::prepare(&path, None, false) {
        Err(crate::import::ImportError::NeedsOcr(_, pages, _, count)) => {
            assert_eq!((pages, count), (vec![1], 1));
        }
        other => panic!("{other:?}"),
    });
}

/// A HEIF file of brand `mif1` only, with one item of `item_type`, primary.
fn generic_heif(item_type: [u8; 4]) -> Vec<u8> {
    fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((8 + body.len()) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }
    let ftyp = boxed(b"ftyp", b"mif1\x00\x00\x00\x00mif1miaf");
    let hdlr = boxed(
        b"hdlr",
        b"\x00\x00\x00\x00\x00\x00\x00\x00pict\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00",
    );
    let pitm = boxed(b"pitm", b"\x00\x00\x00\x00\x00\x01");
    let mut infe = b"\x02\x00\x00\x00\x00\x01\x00\x00".to_vec();
    infe.extend_from_slice(&item_type);
    infe.push(0);
    let mut iinf = b"\x00\x00\x00\x00\x00\x01".to_vec();
    iinf.extend_from_slice(&boxed(b"infe", &infe));
    let mut meta = b"\x00\x00\x00\x00".to_vec();
    for part in [hdlr, pitm, boxed(b"iinf", &iinf)] {
        meta.extend_from_slice(&part);
    }
    [ftyp, boxed(b"meta", &meta), boxed(b"mdat", &[0; 32])].concat()
}

/// A box of `kind` around `body`.
fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = ((8 + body.len()) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out
}

/// A full box (version 0, no flags) of `kind` around `body`.
fn full_box(kind: &[u8; 4], flags: u8, body: &[u8]) -> Vec<u8> {
    boxed(kind, &[&[0, 0, 0, flags][..], body].concat())
}

/// A generic HEIF (`mif1` only) of item 1 of `item_type`, primary, whose
/// `meta` box follows `gap` bytes of `mdat`: past the probe's first read
/// when `gap` is large.
fn generic_heif_meta_last(item_type: [u8; 4], gap: usize) -> Vec<u8> {
    let ftyp = boxed(b"ftyp", b"mif1\x00\x00\x00\x00mif1miaf");
    let hdlr = full_box(
        b"hdlr",
        0,
        b"\x00\x00\x00\x00pict\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00",
    );
    let pitm = full_box(b"pitm", 0, &1u16.to_be_bytes());
    let infe = full_box(b"infe", 0, &[&[0, 1, 0, 0][..], &item_type, &[0]].concat());
    // Version 2 of `infe`.
    let mut infe = infe;
    infe[8] = 2;
    let iinf = full_box(b"iinf", 0, &[&1u16.to_be_bytes()[..], &infe].concat());
    let meta = full_box(b"meta", 0, &[hdlr, pitm, iinf].concat());
    [ftyp, boxed(b"mdat", &vec![0; gap]), meta].concat()
}

/// An alpha plane whose `auxC` association sits in a second `ipma` box
/// (HEIF allows one per version and flags pair) is found as well as one
/// in the first.
#[test]
fn the_alpha_plane_is_found_in_any_ipma_box() {
    use super::alpha_item;
    let file = |split: bool| {
        let ftyp = boxed(b"ftyp", b"heic\x00\x00\x00\x00mif1heic");
        let pitm = full_box(b"pitm", 0, &1u16.to_be_bytes());
        let infe = |id: u16| {
            let mut infe = full_box(
                b"infe",
                0,
                &[&id.to_be_bytes()[..], &[0, 0], b"hvc1", &[0]].concat(),
            );
            infe[8] = 2;
            infe
        };
        let iinf = full_box(
            b"iinf",
            0,
            &[&2u16.to_be_bytes()[..], &infe(1), &infe(2)].concat(),
        );
        // Item 2 is item 1's auxiliary.
        let auxl = boxed(
            b"auxl",
            &[
                &2u16.to_be_bytes()[..],
                &1u16.to_be_bytes(),
                &1u16.to_be_bytes(),
            ]
            .concat(),
        );
        let iref = full_box(b"iref", 0, &auxl);
        let ipco = boxed(
            b"ipco",
            &[
                boxed(b"ispe", &[0; 12]),
                full_box(b"auxC", 0, b"urn:mpeg:hevc:2015:auxid:1\x00"),
            ]
            .concat(),
        );
        // Item 1 has property 1; item 2 has properties 1 and 2 (`auxC`).
        let entry = |item: u16, properties: &[u8]| {
            [
                &item.to_be_bytes()[..],
                &[properties.len() as u8],
                properties,
            ]
            .concat()
        };
        let ipma = |entries: &[Vec<u8>]| {
            full_box(
                b"ipma",
                0,
                &[&(entries.len() as u32).to_be_bytes()[..], &entries.concat()].concat(),
            )
        };
        let associations = if split {
            [ipma(&[entry(1, &[1])]), ipma(&[entry(2, &[1, 2])])].concat()
        } else {
            ipma(&[entry(1, &[1]), entry(2, &[1, 2])])
        };
        let iprp = boxed(b"iprp", &[ipco, associations].concat());
        let meta = full_box(b"meta", 0, &[pitm, iinf, iref, iprp].concat());
        [ftyp, meta].concat()
    };
    assert_eq!(alpha_item(&file(false)), Some(2));
    assert_eq!(alpha_item(&file(true)), Some(2));
}

/// A box whose 64-bit size overflows its offset ends the list, where a
/// sum would overflow.
#[test]
fn an_overflowing_box_size_ends_the_list() {
    let mut data = boxed(b"free", &[0; 8]);
    data.extend_from_slice(&1u32.to_be_bytes());
    data.extend_from_slice(b"meta");
    data.extend_from_slice(&u64::MAX.to_be_bytes());
    data.extend_from_slice(&[0; 16]);
    let kinds: Vec<[u8; 4]> = super::boxes(&data).map(|(kind, _)| kind).collect();
    assert_eq!(kinds, [*b"free"]);
    assert!(!is_heic(
        &[boxed(b"ftyp", b"mif1\x00\x00\x00\x00mif1"), data].concat()
    ));
}

/// A renamed generic HEIF whose `meta` box lies past the first 64 KiB is
/// known by the file probe that admission and the Original ask, reading
/// only its boxes' headers and that box; AV1 stays out. At the import seam
/// it is one page for OCR.
#[test]
fn the_file_probe_finds_a_meta_box_far_into_the_file() {
    use crate::image_pdf::format_of_file;
    let dir = tempfile::tempdir().unwrap();
    let heic = dir.path().join("scan.bin");
    std::fs::write(&heic, generic_heif_meta_last(*b"hvc1", 100 * 1024)).unwrap();
    let avif = dir.path().join("other.bin");
    std::fs::write(&avif, generic_heif_meta_last(*b"av01", 100 * 1024)).unwrap();
    assert_eq!(format_of_file(&heic).unwrap(), Some(ImageFormat::Heic));
    assert_eq!(format_of_file(&avif).unwrap(), None);
    assert!(crate::import::supported_source(&heic));
    let stub = Stub {
        width: 120,
        height: 160,
        orientation: 1,
    };
    with_decoder(stub, || match crate::import::prepare(&heic, None, false) {
        Err(crate::import::ImportError::NeedsOcr(_, pages, _, count)) => {
            assert_eq!((pages, count), (vec![1], 1));
        }
        other => panic!("{other:?}"),
    });
}

/// A HEIF file is held to the 256 MiB cap however far its `meta` box lies:
/// a generic one with 300 MiB of `mdat` before it is refused by the read,
/// having read no more than the cap; and the builder refuses more than the
/// cap again, before any decoder sees it.
#[test]
fn a_heif_file_over_the_cap_is_refused() {
    use super::MAX_FILE_BYTES;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.heif");
    {
        use std::io::{Seek, SeekFrom, Write};
        let small = generic_heif_meta_last(*b"hvc1", 0);
        let (ftyp, meta) = (&small[..24], &small[32..]);
        let gap: u64 = 300 * 1024 * 1024;
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(ftyp).unwrap();
        file.write_all(&(gap as u32 + 8).to_be_bytes()).unwrap();
        file.write_all(b"mdat").unwrap();
        // The `mdat` is a hole: nothing is written to it.
        file.seek(SeekFrom::Current(gap as i64)).unwrap();
        file.write_all(meta).unwrap();
    }
    match crate::import::Source::read(&path) {
        Err(crate::import::ImportError::Image(ImageError::TooLarge)) => {}
        other => panic!("{:?}", other.map(|_| ())),
    }
    // The builder's own check, before the decoder.
    struct Never;
    impl OsDecoder for Never {
        fn decode_heic(&self, _: &[u8]) -> Result<OsDecoded, ImageError> {
            panic!("decoded past the cap");
        }
    }
    let mut bytes = vec![0u8; MAX_FILE_BYTES as usize + 1];
    bytes[..64].copy_from_slice(&read("page-upright.heic")[..64]);
    let refused = with_decoder(Never, || super::page(&bytes).err());
    assert!(matches!(refused, Some(ImageError::TooLarge)), "{refused:?}");
}

/// `name` claiming to be 40,000 pixels square: every `ispe` box (the
/// image's stored size) rewritten, the coded data left as it is.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn oversized(name: &str) -> Vec<u8> {
    let mut bytes = read(name);
    let mut at = 0;
    while let Some(found) = bytes[at..].windows(4).position(|w| w == b"ispe") {
        let size = at + found + 4 + 4;
        bytes[size..size + 4].copy_from_slice(&40_000u32.to_be_bytes());
        bytes[size + 4..size + 8].copy_from_slice(&40_000u32.to_be_bytes());
        at = size + 8;
    }
    assert!(at > 0, "{name} has an ispe box");
    bytes
}

/// The GPS position, camera and date in the photo never reach the PDF,
/// so OCR's Markdown, read from the PDF, can't hold them either.
#[test]
fn a_heic_photo_s_metadata_never_reaches_the_pdf() {
    let bytes = read("page-gps.heic");
    // The fixture does carry them.
    let has = |haystack: &[u8], needle: &[u8]| haystack.windows(needle.len()).any(|w| w == needle);
    assert!(has(&bytes, b"Fixture Camera Co"));
    let stub = Stub {
        width: 1200,
        height: 1600,
        orientation: 1,
    };
    with_decoder(stub, || {
        let built = build(&[&bytes], DEFAULT_DPI).unwrap();
        for leaked in [
            b"Fixture Camera Co".as_slice(),
            b"Model F1",
            b"2031:03:14",
            b"Exif",
            b"ftyp",
        ] {
            assert!(
                !has(&built.pdf, leaked),
                "{}",
                String::from_utf8_lossy(leaked)
            );
        }
    });
}

/// A PC without the extensions gets a card naming them, through the
/// import's own error.
#[test]
fn a_missing_codec_names_the_extensions() {
    for (missing, named) in [
        (
            MissingCodec::HeifExtension,
            &[
                "HEIF Image Extensions",
                "HEVC Video Extensions",
                "Microsoft Store",
            ][..],
        ),
        (
            MissingCodec::HevcExtension,
            &["HEVC Video Extensions", "Microsoft Store"][..],
        ),
    ] {
        let error = with_decoder(Missing(missing), || {
            crate::import::prepare(&fixture("page-upright.heic"), None, false).unwrap_err()
        });
        assert!(
            matches!(&error, crate::import::ImportError::Image(ImageError::HeicCodecMissing(m)) if *m == missing),
            "{error:?}"
        );
        let message = error.to_string();
        for name in named {
            assert!(message.contains(name), "{message}");
        }
    }
}

/// The cards' exact words. HEVC Video Extensions is a paid Store app, so
/// neither says "free".
#[test]
fn the_missing_codec_cards_read_as_written() {
    let both = "Opening HEIC photos on Windows needs Microsoft's HEIF Image Extensions and HEVC \
                Video Extensions. Install them from the Microsoft Store, then import the photo \
                again.";
    let hevc = "Opening HEIC photos on Windows needs Microsoft's HEVC Video Extensions. Install \
                them from the Microsoft Store, then import the photo again.";
    assert_eq!(
        ImageError::HeicCodecMissing(MissingCodec::HeifExtension).to_string(),
        both
    );
    assert_eq!(
        ImageError::HeicCodecMissing(MissingCodec::HevcExtension).to_string(),
        hevc
    );
}

/// Where the red arrow in a decoded fixture is: the quadrant with the most
/// red pixels, once the orientation is applied.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn arrow_corner(decoded: OsDecoded) -> (u32, u32, &'static str) {
    let mut image = decoded.image;
    image.apply_orientation(image::metadata::Orientation::from_exif(decoded.orientation).unwrap());
    let image = image.into_rgb8();
    let (width, height) = image.dimensions();
    let mut counts = [0u32; 4];
    for (x, y, pixel) in image.enumerate_pixels() {
        let [r, g, b] = pixel.0;
        if r > 170 && g < 100 && b < 100 {
            counts[usize::from(x >= width / 2) + 2 * usize::from(y >= height / 2)] += 1;
        }
    }
    let most = (0..4).max_by_key(|&index| counts[index]).unwrap();
    let corner = ["top-left", "top-right", "bottom-left", "bottom-right"][most];
    (width, height, corner)
}

/// WIC's own "no decoder" path: asked for a container format no decoder
/// implements, WIC says what a PC without the HEIF extension says when
/// asked for HEIF, and the card names the extensions.
#[cfg(target_os = "windows")]
#[test]
fn wic_without_a_heif_decoder_names_the_extension() {
    let nobody = windows::core::GUID::from_u128(0x6d64_6f63_0000_4000_8000_0000_0000_0001);
    let error = super::wic::decode(&read("page-upright.heic"), &nobody)
        .err()
        .expect("no decoder for that format");
    assert!(
        matches!(
            error,
            ImageError::HeicCodecMissing(MissingCodec::HeifExtension)
        ),
        "{error:?}"
    );
    assert!(error.to_string().contains("HEIF Image Extensions"));
}

/// The real fixtures through WIC, on a PC with Microsoft's HEIF and HEVC
/// extensions (CI's Windows runners have neither): each comes out a
/// 1200 × 1600 page with its arrow top-left, and builds without its
/// metadata.
#[cfg(target_os = "windows")]
#[test]
#[ignore = "needs Microsoft's HEIF and HEVC extensions"]
fn wic_decodes_the_fixtures_upright() {
    for name in FIXTURES {
        let decoded = super::wic::Decoder.decode_heic(&read(name)).unwrap();
        assert_eq!(arrow_corner(decoded), (1200, 1600, "top-left"), "{name}");
        let built = build(&[&read(name)], DEFAULT_DPI).unwrap();
        let pdf = &built.pdf;
        assert!(
            !pdf.windows(17).any(|w| w == b"Fixture Camera Co"),
            "{name}"
        );
    }
}

/// The real fixtures through ImageIO: upright 1200 × 1600 pages, arrow
/// top-left (macOS decodes HEIC out of the box).
#[cfg(target_os = "macos")]
#[test]
fn image_io_decodes_the_fixtures_upright() {
    for name in FIXTURES {
        let decoded = super::image_io::Decoder.decode_heic(&read(name)).unwrap();
        assert_eq!(arrow_corner(decoded), (1200, 1600, "top-left"), "{name}");
        let built = build(&[&read(name)], DEFAULT_DPI).unwrap();
        assert!(
            !built.pdf.windows(17).any(|w| w == b"Fixture Camera Co"),
            "{name}"
        );
    }
}

/// Whether the page built from `bytes` shows `expected` in column `x`,
/// row 32, give or take the encoders' rounding (the HEVC fixture's colours
/// are decoded within 20 of what was encoded).
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn shows(bytes: &[u8], x: u32, expected: [u8; 3]) -> bool {
    let built = build(&[bytes], DEFAULT_DPI).unwrap();
    let image = image::load_from_memory(jpeg_in(&built.pdf))
        .unwrap()
        .into_rgb8();
    let stored = image.get_pixel(x, 32).0;
    stored
        .iter()
        .zip(expected)
        .all(|(a, b)| a.abs_diff(b) <= 20)
}

/// The real transparent fixture through WIC: what lies under its
/// transparent pixels (black) never shows; the page is white there.
#[cfg(target_os = "windows")]
#[test]
#[ignore = "needs Microsoft's HEIF and HEVC extensions"]
fn wic_puts_a_transparent_heic_on_white() {
    let bytes = read("transparent.heic");
    for x in [8, 40, 56] {
        assert!(shows(&bytes, x, on_white(x)), "{x}");
    }
}

/// The real two-image fixture through WIC: the primary image (red), not
/// the first (blue).
#[cfg(target_os = "windows")]
#[test]
#[ignore = "needs Microsoft's HEIF and HEVC extensions"]
fn wic_decodes_the_primary_image() {
    assert!(shows(&read("two-images.heic"), 32, [220, 30, 30]));
}

/// A HEIC claiming 40,000 pixels square is refused as too large, never
/// decoded at that size.
#[cfg(target_os = "windows")]
#[test]
#[ignore = "needs Microsoft's HEIF and HEVC extensions"]
fn wic_refuses_an_oversized_heic() {
    let error = super::wic::Decoder
        .decode_heic(&oversized("page-upright.heic"))
        .err();
    assert!(
        matches!(error, Some(ImageError::TooLarge | ImageError::Unreadable)),
        "{error:?}"
    );
}

/// The real transparent fixture through ImageIO, drawn on white.
#[cfg(target_os = "macos")]
#[test]
fn image_io_puts_a_transparent_heic_on_white() {
    let bytes = read("transparent.heic");
    for x in [8, 40, 56] {
        assert!(shows(&bytes, x, on_white(x)), "{x}");
    }
}

/// ImageIO decodes the image the file names primary (red), not its first
/// (blue).
#[cfg(target_os = "macos")]
#[test]
fn image_io_decodes_the_primary_image() {
    assert!(shows(&read("two-images.heic"), 32, [220, 30, 30]));
}

/// A HEIC whose `ispe` boxes claim 40,000 pixels square is refused, never
/// decoded at that size. (ImageIO refuses the rewritten file as damaged:
/// its tiles no longer add up. No encoder here writes a real HEIC over
/// the limits, so `sizes_over_the_limits_are_refused` pins the check
/// ImageIO's stored size goes through.)
#[cfg(target_os = "macos")]
#[test]
fn image_io_refuses_an_oversized_heic() {
    let error = super::image_io::Decoder
        .decode_heic(&oversized("page-upright.heic"))
        .err();
    assert!(
        matches!(error, Some(ImageError::TooLarge | ImageError::Unreadable)),
        "{error:?}"
    );
}

/// At the import seam with ImageIO: each real fixture is one page for OCR.
#[cfg(target_os = "macos")]
#[test]
fn image_io_imports_each_fixture_as_one_page_for_ocr() {
    for name in FIXTURES {
        match crate::import::prepare(&fixture(name), None, false) {
            Err(crate::import::ImportError::NeedsOcr(_, pages, _, count)) => {
                assert_eq!((pages, count), (vec![1], 1), "{name}");
            }
            other => panic!("{name}: {other:?}"),
        }
    }
}

/// Without a decoder built in (Linux), a HEIC photo says so.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
#[test]
fn elsewhere_heic_is_unsupported() {
    assert!(matches!(
        build(&[&read("page-upright.heic")], DEFAULT_DPI),
        Err(ImageError::HeicUnsupported)
    ));
}

#[cfg(target_os = "windows")]
const CHILD_HEIC: &str = "MDOC_TEST_HEIC_IMPORT";

/// Nothing decoded reaches disk: a HEIC import through WIC and the
/// extensions, in a child process whose temporary folder is an empty one,
/// leaves that folder empty.
#[cfg(target_os = "windows")]
#[test]
#[ignore = "needs Microsoft's HEIF and HEVC extensions"]
fn a_heic_import_writes_nothing_to_the_temporary_folder() {
    let dir = tempfile::tempdir().unwrap();
    let temp = dir.path().join("temp");
    std::fs::create_dir(&temp).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "image_pdf::heic::tests::child_imports_a_heic",
            "--include-ignored",
        ])
        .env(CHILD_HEIC, fixture("page-gps.heic"))
        .env("TMP", &temp)
        .env("TEMP", &temp)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("1 passed"), "{stdout}");
    let left: Vec<_> = std::fs::read_dir(&temp).unwrap().collect();
    assert!(left.is_empty(), "{left:?}");
}

/// The child of the test above.
#[cfg(target_os = "windows")]
#[test]
#[ignore = "run by a_heic_import_writes_nothing_to_the_temporary_folder"]
fn child_imports_a_heic() {
    let path = std::env::var_os(CHILD_HEIC).expect("run by its parent test");
    let temp = std::env::var_os("TMP").unwrap();
    assert_eq!(
        std::fs::canonicalize(std::env::temp_dir()).unwrap(),
        std::fs::canonicalize(temp).unwrap()
    );
    match crate::import::prepare(Path::new(&path), None, false) {
        Err(crate::import::ImportError::NeedsOcr(_, pages, _, _)) => assert_eq!(pages, [1]),
        other => panic!("{other:?}"),
    }
}
