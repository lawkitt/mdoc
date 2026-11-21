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
            image: page(self.width, self.height),
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
/// the page turns, the stored 1600 × 1200 pixels show as a 1200 × 1600
/// page.
#[test]
fn an_oriented_heic_is_turned_by_the_page() {
    let stub = Stub {
        width: 1600,
        height: 1200,
        orientation: 6,
    };
    with_decoder(stub, || {
        let built = build(&[&read("page-orientation-6.heic")], 72).unwrap();
        let pdf = text(&built.pdf);
        assert!(pdf.contains("/MediaBox [0 0 1200 1600]"), "{pdf}");
        assert!(pdf.contains("/Width 1600 /Height 1200"), "{pdf}");
        assert!(pdf.contains("q 0 -1600 1200 0 0 1600 cm"), "{pdf}");
    });
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
    let mut image = image::DynamicImage::ImageRgb8(decoded.image);
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
