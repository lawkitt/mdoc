//! The image builder at its seam: image bytes in, PDF out. Every image is
//! made here, with fictional content.

use super::*;
use image::{DynamicImage, ImageBuffer, Rgb, Rgba};

/// What the writer put on a one-page PDF.
#[derive(Debug)]
struct Built {
    media_box: (f64, f64),
    matrix: [f64; 6],
    width: u32,
    height: u32,
    color: String,
    filter: String,
    data: Vec<u8>,
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    find(haystack, needle).is_some()
}

/// The text after `key` up to the next delimiter.
fn value_after<'a>(text: &'a str, key: &str) -> &'a str {
    let start = text.find(key).unwrap_or_else(|| panic!("{key} in {text}")) + key.len();
    let rest = text[start..].trim_start();
    let end = rest.find([' ', '>', ']', '/']).unwrap_or(rest.len());
    &rest[..end]
}

fn numbers(text: &str) -> Vec<f64> {
    text.split_whitespace()
        .map(|number| number.parse().unwrap())
        .collect()
}

/// The single page of `pdf`.
fn page(pdf: &[u8]) -> Built {
    let text = String::from_utf8_lossy(pdf);
    assert!(text.starts_with("%PDF-1.4"));
    assert!(text.contains("/Count 1 "), "{text}");
    let media = &text[text.find("/MediaBox [").unwrap() + 11..];
    let media = numbers(&media[..media.find(']').unwrap()]);
    let image_head = &text[text.find("/Subtype /Image").unwrap()..];
    let length: usize = value_after(image_head, "/Length").parse().unwrap();
    let stream_start = find(pdf, b"/Subtype /Image").unwrap()
        + find(&pdf[find(pdf, b"/Subtype /Image").unwrap()..], b"stream\n").unwrap()
        + 7;
    let content = &text[text.rfind("q ").unwrap() + 2..text.rfind(" cm").unwrap()];
    let matrix: [f64; 6] = numbers(content).try_into().unwrap();
    Built {
        media_box: (media[2], media[3]),
        matrix,
        width: value_after(image_head, "/Width").parse().unwrap(),
        height: value_after(image_head, "/Height").parse().unwrap(),
        color: value_after(image_head, "/ColorSpace /").to_owned(),
        filter: value_after(image_head, "/Filter /").to_owned(),
        data: pdf[stream_start..stream_start + length].to_vec(),
    }
}

/// `bytes` saved as `name` and taken through the file import seam: the
/// import asks OCR for its one page, and reading the file as the import
/// does gives the PDF it built and the animation frames it left out.
fn through_import(name: &str, bytes: &[u8]) -> ImagePdf {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    match crate::import::prepare(&path, None, false) {
        Err(crate::import::ImportError::NeedsOcr(_, pages, _, count)) => {
            assert_eq!((pages, count), (vec![1], 1), "{name}");
        }
        other => panic!("{name}: {other:?}"),
    }
    let source = crate::import::Source::read(&path).unwrap();
    let (pdf, frames_left_out) = source.image_pdf(DEFAULT_DPI).unwrap().unwrap();
    ImagePdf {
        pdf: pdf.bytes().to_vec(),
        pages: 1,
        frames_left_out,
    }
}

fn build_one(bytes: &[u8]) -> ImagePdf {
    build(&[bytes], DEFAULT_DPI).unwrap()
}

/// A JPEG `width` by `height` in `color`, as an encoder writes it (with
/// its own JFIF APP0 segment).
fn jpeg(width: u32, height: u32, color: [u8; 3]) -> Vec<u8> {
    let image = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(width, height, Rgb(color)));
    let mut bytes = Vec::new();
    image
        .write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut bytes, 90,
        ))
        .unwrap();
    bytes
}

/// `jpeg` without its first segment, the encoder's JFIF APP0.
fn without_app0(jpeg: &[u8]) -> Vec<u8> {
    assert_eq!(&jpeg[2..4], &[0xff, 0xe0]);
    let length = usize::from(u16::from_be_bytes([jpeg[4], jpeg[5]]));
    [&jpeg[..2], &jpeg[4 + length..]].concat()
}

/// `jpeg` with `segments` inserted right after its start-of-image marker.
fn with_segments(jpeg: &[u8], segments: &[Vec<u8>]) -> Vec<u8> {
    let mut out = jpeg[..2].to_vec();
    for segment in segments {
        out.extend_from_slice(segment);
    }
    out.extend_from_slice(&jpeg[2..]);
    out
}

fn segment(marker: u8, body: &[u8]) -> Vec<u8> {
    let mut segment = vec![0xff, marker];
    segment.extend_from_slice(&u16::try_from(body.len() + 2).unwrap().to_be_bytes());
    segment.extend_from_slice(body);
    segment
}

/// The GPS latitude 50° 27′ 0″ as three rationals: easy to find in bytes.
const LATITUDE: [u8; 24] = [
    50, 0, 0, 0, 1, 0, 0, 0, 27, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0,
];

/// A little-endian TIFF block as EXIF carries it: a fictional camera make,
/// an optional orientation and a GPS position.
fn exif_tiff(orientation: Option<u16>) -> Vec<u8> {
    let mut tiff = b"II*\x00".to_vec();
    tiff.extend_from_slice(&8u32.to_le_bytes());
    let entries: u16 = if orientation.is_some() { 3 } else { 2 };
    let ifd_end = 8 + 2 + 12 * u32::from(entries) + 4;
    let make_at = ifd_end;
    let make = b"FictionalCam\x00\x00";
    let gps_at = make_at + make.len() as u32;
    let latitude_at = gps_at + 2 + 2 * 12 + 4;
    let entry = |tiff: &mut Vec<u8>, tag: u16, kind: u16, count: u32, value: u32| {
        tiff.extend_from_slice(&tag.to_le_bytes());
        tiff.extend_from_slice(&kind.to_le_bytes());
        tiff.extend_from_slice(&count.to_le_bytes());
        tiff.extend_from_slice(&value.to_le_bytes());
    };
    tiff.extend_from_slice(&entries.to_le_bytes());
    entry(&mut tiff, 0x010f, 2, 13, make_at);
    if let Some(orientation) = orientation {
        entry(&mut tiff, 0x0112, 3, 1, u32::from(orientation));
    }
    entry(&mut tiff, 0x8825, 4, 1, gps_at);
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(make);
    // GPS: latitude reference N, then the latitude.
    tiff.extend_from_slice(&2u16.to_le_bytes());
    entry(&mut tiff, 0x0001, 2, 2, u32::from(b'N'));
    entry(&mut tiff, 0x0002, 5, 3, latitude_at);
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(&LATITUDE);
    tiff
}

fn app1_exif(orientation: Option<u16>) -> Vec<u8> {
    segment(
        0xe1,
        &[b"Exif\x00\x00".as_slice(), &exif_tiff(orientation)].concat(),
    )
}

fn png(image: &DynamicImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}

fn inflate(data: &[u8]) -> Vec<u8> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .read_to_end(&mut out)
        .unwrap();
    out
}

/// Where the unit square's corner (`u`, `v`) lands on the page, as a
/// corner name ("top-left" …), `v` = 1 being the image's first row.
fn corner(matrix: [f64; 6], (page_width, page_height): (f64, f64), u: f64, v: f64) -> String {
    let [a, b, c, d, e, f] = matrix;
    let (x, y) = (a * u + c * v + e, b * u + d * v + f);
    let near = |value: f64, target: f64| (value - target).abs() < 1e-6;
    let horizontal = if near(x, 0.) {
        "left"
    } else {
        assert!(near(x, page_width), "{x}");
        "right"
    };
    let vertical = if near(y, 0.) {
        "bottom"
    } else {
        assert!(near(y, page_height), "{y}");
        "top"
    };
    format!("{vertical}-{horizontal}")
}

/// Each EXIF orientation draws the stored image upright: the stored
/// top-left and top-right pixels land where a viewer shows them, and a
/// quarter turn swaps the page's sides. The JPEG passes through, so the
/// orientation is the matrix's alone.
#[test]
fn every_exif_orientation_is_drawn_upright_by_the_page_matrix() {
    // (orientation, stored top-left lands, stored top-right lands)
    let expected = [
        (1, "top-left", "top-right"),
        (2, "top-right", "top-left"),
        (3, "bottom-right", "bottom-left"),
        (4, "bottom-left", "bottom-right"),
        (5, "top-left", "bottom-left"),
        (6, "top-right", "bottom-right"),
        (7, "bottom-right", "top-right"),
        (8, "bottom-left", "top-left"),
    ];
    let source = jpeg(40, 20, [200, 30, 30]);
    for (orientation, top_left, top_right) in expected {
        let photo = with_segments(&source, &[app1_exif(Some(orientation))]);
        let built = build_one(&photo);
        let page = page(&built.pdf);
        let (stored_width, stored_height) = (40. * 72. / 150., 20. * 72. / 150.);
        let expected_box = if orientation >= 5 {
            (stored_height, stored_width)
        } else {
            (stored_width, stored_height)
        };
        assert_eq!(page.media_box, expected_box, "orientation {orientation}");
        assert_eq!((page.width, page.height), (40, 20));
        assert_eq!(page.filter, "DCTDecode", "orientation {orientation}");
        assert_eq!(
            corner(page.matrix, page.media_box, 0., 1.),
            top_left,
            "orientation {orientation}"
        );
        assert_eq!(
            corner(page.matrix, page.media_box, 1., 1.),
            top_right,
            "orientation {orientation}"
        );
    }
}

/// A decoded image keeps its orientation the same way: a PNG with an
/// eXIf chunk saying "turn a quarter clockwise".
#[test]
fn a_decoded_image_is_drawn_upright_too() {
    let source = png(&DynamicImage::ImageRgb8(ImageBuffer::from_pixel(
        40,
        20,
        Rgb([10, 120, 200]),
    )));
    // eXIf goes before the image data: right after IHDR.
    let tiff = exif_tiff(Some(6));
    let mut chunk = (tiff.len() as u32).to_be_bytes().to_vec();
    let mut crc = flate2::Crc::new();
    crc.update(b"eXIf");
    crc.update(&tiff);
    chunk.extend_from_slice(b"eXIf");
    chunk.extend_from_slice(&tiff);
    chunk.extend_from_slice(&crc.sum().to_be_bytes());
    let ihdr_end = 8 + 8 + 13 + 4;
    let photo = [&source[..ihdr_end], &chunk, &source[ihdr_end..]].concat();
    let page = page(&build_one(&photo).pdf);
    assert_eq!(page.filter, "FlateDecode");
    assert_eq!(page.media_box, (20. * 72. / 150., 40. * 72. / 150.));
    assert_eq!(corner(page.matrix, page.media_box, 0., 1.), "top-right");
    // The camera make in the eXIf chunk doesn't travel either.
    assert!(!contains(&build_one(&photo).pdf, b"FictionalCam"));
}

/// A JPEG at or under 4,500 pixels passes through byte for byte, without
/// its metadata: EXIF with GPS and camera, XMP, ICC, Photoshop resources,
/// comments, and what follows its end (a phone's appended preview). Adobe's
/// APP14 stays, since the decoder needs it.
#[test]
fn a_gps_tagged_jpeg_passes_through_without_any_metadata() {
    let clean = jpeg(64, 48, [90, 160, 60]);
    let adobe = segment(0xee, b"Adobe\x00\x64\x00\x00\x00\x00\x01");
    let metadata = [
        app1_exif(Some(1)),
        segment(
            0xe1,
            b"http://ns.adobe.com/xap/1.0/\x00<x:xmpmeta>Fictional Street 12</x:xmpmeta>",
        ),
        segment(0xe2, b"ICC_PROFILE\x00\x01\x01fictional-profile"),
        segment(0xed, b"Photoshop 3.0\x008BIM fictional caption"),
        segment(0xfe, b"Taken at Fictional Street 12"),
    ];
    let mut segments = metadata.to_vec();
    segments.push(adobe.clone());
    let mut photo = with_segments(&clean, &segments);
    photo.extend_from_slice(b"MPF-PREVIEW-AFTER-EOI");
    let built = build_one(&photo);
    let page = page(&built.pdf);
    assert_eq!(page.filter, "DCTDecode");
    assert_eq!(page.color, "DeviceRGB");
    // Byte for byte the encoder's file, less its JFIF segment, with the
    // Adobe segment where it was.
    assert_eq!(page.data, with_segments(&without_app0(&clean), &[adobe]));
    for leaked in [
        b"Exif".as_slice(),
        b"FictionalCam",
        &LATITUDE,
        b"xmpmeta",
        b"ICC_PROFILE",
        b"8BIM",
        b"Fictional Street",
        b"MPF-PREVIEW",
        b"JFIF",
    ] {
        assert!(
            !contains(&built.pdf, leaked),
            "{}",
            String::from_utf8_lossy(leaked)
        );
    }
}

/// The boundary: a 4,500-pixel side passes through; one more is decoded,
/// scaled to 4,500 and stored as a JPEG again.
#[test]
fn an_oversized_photo_is_scaled_down_and_only_it() {
    let at_limit = jpeg(MAX_LONG_SIDE, 16, [30, 60, 90]);
    let page_at_limit = page(&through_import("at-limit.jpg", &at_limit).pdf);
    assert_eq!(page_at_limit.data, without_app0(&at_limit));
    assert_eq!((page_at_limit.width, page_at_limit.height), (4500, 16));

    let oversized = jpeg(4800, 320, [30, 60, 90]);
    let page = page(&through_import("oversized.jpg", &oversized).pdf);
    assert_eq!((page.width, page.height), (4500, 300));
    // The sheet stays the photo's own: 4,800 by 320 pixels at 150 dpi.
    assert_eq!(page.media_box, (4800. * 72. / 150., 320. * 72. / 150.));
    assert_eq!(page.filter, "DCTDecode");
    assert_ne!(page.data, without_app0(&oversized));
    let decoded = image::load_from_memory(&page.data).unwrap().into_rgb8();
    assert_eq!(decoded.dimensions(), (4500, 300));
    let pixel = decoded.get_pixel(2000, 150);
    for (channel, expected) in pixel.0.iter().zip([30u8, 60, 90]) {
        assert!(channel.abs_diff(expected) <= 6, "{pixel:?}");
    }
    // Pixels are scaled down too, as a lossless source stays lossless.
    let wide_png = png(&DynamicImage::ImageRgb8(ImageBuffer::from_pixel(
        9000,
        20,
        Rgb([255, 255, 255]),
    )));
    let page = self::page(&through_import("wide.png", &wide_png).pdf);
    assert_eq!(
        (page.width, page.height, page.filter.as_str()),
        (4500, 10, "FlateDecode")
    );
}

/// Transparency goes on white: a clear pixel becomes white, a half-clear
/// red one pink. A lossless source stays lossless (Flate).
#[test]
fn transparency_is_composited_on_white() {
    let rgba = ImageBuffer::from_fn(2, 1, |x, _| {
        if x == 0 {
            Rgba([255, 0, 0, 0])
        } else {
            Rgba([255, 0, 0, 128])
        }
    });
    let page = page(&through_import("alpha.png", &png(&DynamicImage::ImageRgba8(rgba))).pdf);
    assert_eq!(
        (page.filter.as_str(), page.color.as_str()),
        ("FlateDecode", "DeviceRGB")
    );
    assert_eq!(inflate(&page.data), [255, 255, 255, 255, 127, 127]);
    // Gray with alpha stays gray.
    let gray = ImageBuffer::from_fn(2, 1, |x, _| {
        image::LumaA([0u8, if x == 0 { 0 } else { 255 }])
    });
    let page = self::page(&through_import("gray.png", &png(&DynamicImage::ImageLumaA8(gray))).pdf);
    assert_eq!(page.color, "DeviceGray");
    assert_eq!(inflate(&page.data), [255, 0]);
}

/// 16-bit samples become 8-bit.
#[test]
fn sixteen_bit_images_become_eight_bit() {
    let deep = ImageBuffer::from_fn(2, 1, |x, _| {
        if x == 0 {
            Rgb([65535u16, 0, 0])
        } else {
            Rgb([0u16, 32896, 65535])
        }
    });
    let built = through_import("deep.png", &png(&DynamicImage::ImageRgb16(deep)));
    assert!(String::from_utf8_lossy(&built.pdf).contains("/BitsPerComponent 8 "));
    assert_eq!(inflate(&page(&built.pdf).data), [255, 0, 0, 0, 128, 255]);
}

/// A minimal 8×8 baseline CMYK JPEG in one flat colour, the way Adobe
/// writes CMYK (an APP14 segment, inverted samples): each component is one
/// block holding only its DC value. Samples are 0 or 255.
fn cmyk_jpeg(stored: [u8; 4]) -> Vec<u8> {
    let mut jpeg = vec![0xff, 0xd8];
    jpeg.extend(segment(0xee, b"Adobe\x00\x64\x00\x00\x00\x00\x00"));
    // One quantization table: 16 for the DC term, so a flat block of 0 or
    // 255 is a DC of -64 or 64 (category 7).
    let mut dqt = vec![0x00, 16];
    dqt.extend([1u8; 63]);
    jpeg.extend(segment(0xdb, &dqt));
    let mut sof = vec![8, 0, 8, 0, 8, 4];
    for id in 1..=4 {
        sof.extend([id, 0x11, 0]);
    }
    jpeg.extend(segment(0xc0, &sof));
    // DC table: category 7 is the only code ("0"); AC table: end of block
    // is the only code ("0").
    let mut dc = vec![0x00, 1];
    dc.extend([0u8; 15]);
    dc.push(7);
    jpeg.extend(segment(0xc4, &dc));
    let mut ac = vec![0x10, 1];
    ac.extend([0u8; 15]);
    ac.push(0);
    jpeg.extend(segment(0xc4, &ac));
    let mut sos = vec![4];
    for id in 1..=4 {
        sos.extend([id, 0x00]);
    }
    sos.extend([0, 63, 0]);
    jpeg.extend(segment(0xda, &sos));
    // Per block: DC code, 7 magnitude bits (64 or its complement), EOB.
    let mut bits = Vec::new();
    for sample in stored {
        bits.push(0);
        let magnitude: u8 = if sample >= 128 {
            0b100_0000
        } else {
            0b011_1111
        };
        bits.extend((0..7).rev().map(|bit| (magnitude >> bit) & 1));
        bits.push(0);
    }
    while bits.len() % 8 != 0 {
        bits.push(1);
    }
    for byte in bits.chunks(8) {
        let byte = byte.iter().fold(0u8, |acc, bit| acc << 1 | bit);
        jpeg.push(byte);
        if byte == 0xff {
            jpeg.push(0);
        }
    }
    jpeg.extend([0xff, 0xd9]);
    jpeg
}

/// CMYK can't pass through: it is decoded to RGB and stored as a JPEG.
#[test]
fn cmyk_becomes_rgb() {
    // Red in CMYK is no cyan, full magenta and yellow, no black; Adobe
    // stores the samples inverted.
    let red = cmyk_jpeg([255, 0, 0, 255]);
    let page = page(&through_import("cmyk.jpg", &red).pdf);
    assert_eq!(
        (page.filter.as_str(), page.color.as_str()),
        ("DCTDecode", "DeviceRGB")
    );
    let decoded = image::load_from_memory(&page.data).unwrap().into_rgb8();
    assert_eq!(decoded.dimensions(), (8, 8));
    let pixel = decoded.get_pixel(4, 4).0;
    assert!(
        pixel[0] > 200 && pixel[1] < 60 && pixel[2] < 60,
        "{pixel:?}"
    );
}

fn gif(colors: &[[u8; 4]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        encoder
            .encode_frames(
                colors
                    .iter()
                    .map(|&color| image::Frame::new(ImageBuffer::from_pixel(4, 4, Rgba(color)))),
            )
            .unwrap();
    }
    bytes
}

/// An animated GIF gives its first frame; the others are counted.
#[test]
fn an_animated_gif_gives_its_first_frame() {
    let animated = gif(&[[200, 0, 0, 255], [0, 200, 0, 255], [0, 0, 200, 255]]);
    let built = through_import("animated.gif", &animated);
    assert_eq!(built.frames_left_out, 2);
    let page = page(&built.pdf);
    assert_eq!(page.filter, "FlateDecode");
    assert_eq!(&inflate(&page.data)[..3], &[200, 0, 0]);
    assert_eq!(build_one(&gif(&[[0, 0, 0, 255]])).frames_left_out, 0);
}

/// A still lossless WebP, as image's encoder writes it.
fn webp_still(color: [u8; 4]) -> Vec<u8> {
    let mut bytes = Vec::new();
    image::codecs::webp::WebPEncoder::new_lossless(&mut bytes)
        .encode(&[color; 16].concat(), 4, 4, image::ExtendedColorType::Rgba8)
        .unwrap();
    bytes
}

/// An animated WebP whose frames are `colors`, each a lossless 4×4 frame.
fn webp_animated(colors: &[[u8; 4]]) -> Vec<u8> {
    let chunk = |fourcc: &[u8], data: &[u8]| {
        let mut chunk = fourcc.to_vec();
        chunk.extend_from_slice(&(data.len() as u32).to_le_bytes());
        chunk.extend_from_slice(data);
        if data.len() % 2 == 1 {
            chunk.push(0);
        }
        chunk
    };
    let three = |value: u32| value.to_le_bytes()[..3].to_vec();
    // Canvas 4×4, animation and alpha flags.
    let mut vp8x = vec![0x12, 0, 0, 0];
    vp8x.extend(three(3));
    vp8x.extend(three(3));
    let mut body = b"WEBP".to_vec();
    body.extend(chunk(b"VP8X", &vp8x));
    body.extend(chunk(b"ANIM", &[255, 255, 255, 255, 0, 0]));
    for &color in colors {
        let still = webp_still(color);
        // The still's VP8L chunk, as the frame's data.
        let frame_data = &still[12..];
        let mut anmf = Vec::new();
        anmf.extend(three(0));
        anmf.extend(three(0));
        anmf.extend(three(3));
        anmf.extend(three(3));
        anmf.extend(three(100));
        anmf.push(0);
        anmf.extend_from_slice(frame_data);
        body.extend(chunk(b"ANMF", &anmf));
    }
    let mut webp = b"RIFF".to_vec();
    webp.extend_from_slice(&(body.len() as u32).to_le_bytes());
    webp.extend(body);
    webp
}

/// An animated WebP gives its first frame too, and a photo format is
/// stored as a JPEG.
#[test]
fn an_animated_webp_gives_its_first_frame() {
    let built = through_import(
        "animated.webp",
        &webp_animated(&[[200, 0, 0, 255], [0, 200, 0, 255], [0, 0, 200, 255]]),
    );
    assert_eq!(built.frames_left_out, 2);
    let page = page(&built.pdf);
    assert_eq!(page.filter, "DCTDecode");
    let first = image::load_from_memory(&page.data).unwrap().into_rgb8();
    let pixel = first.get_pixel(1, 1).0;
    assert!(
        pixel[0] > 160 && pixel[1] < 50 && pixel[2] < 50,
        "{pixel:?}"
    );
    assert_eq!(build_one(&webp_still([1, 2, 3, 255])).frames_left_out, 0);
}

/// A PNG header claiming `width` by `height` RGBA pixels, with a token of
/// image data: what a decompression bomb looks like before decoding.
fn png_claiming(width: u32, height: u32) -> Vec<u8> {
    let chunk = |kind: &[u8], data: &[u8]| {
        let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
        let mut crc = flate2::Crc::new();
        crc.update(kind);
        crc.update(data);
        chunk.extend_from_slice(kind);
        chunk.extend_from_slice(data);
        chunk.extend_from_slice(&crc.sum().to_be_bytes());
        chunk
    };
    let mut ihdr = width.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(chunk(b"IHDR", &ihdr));
    png.extend(chunk(b"IDAT", &deflate(&[0; 64])));
    png.extend(chunk(b"IEND", &[]));
    png
}

/// The limits refuse a bomb before decoding it: by its sides, and by the
/// memory its pixels would take.
#[test]
fn limits_refuse_a_decompression_bomb() {
    for (width, height) in [(100_000, 100_000), (20_000, 20_000)] {
        assert!(
            matches!(
                build(&[&png_claiming(width, height)], DEFAULT_DPI),
                Err(ImageError::TooLarge)
            ),
            "{width}×{height}"
        );
    }
}

#[test]
fn damaged_images_are_unreadable() {
    for bytes in [
        b"\xff\xd8\xff\xe0 not really".as_slice(),
        b"\x89PNG\r\n\x1a\n broken",
        b"GIF89a",
        b"plain text",
    ] {
        assert!(matches!(
            build(&[bytes], DEFAULT_DPI),
            Err(ImageError::Unreadable)
        ));
    }
}

/// Several images make one document, a page each, in order: the phones'
/// multi-photo import (#1024 decision 15).
#[test]
fn several_images_make_one_page_each() {
    let first = jpeg(30, 10, [0, 0, 0]);
    let second = png(&DynamicImage::ImageRgb8(ImageBuffer::from_pixel(
        10,
        30,
        Rgb([255, 255, 255]),
    )));
    let built = build(&[&first, &second], 72).unwrap();
    assert_eq!(built.pages, 2);
    let text = String::from_utf8_lossy(&built.pdf);
    assert!(text.contains("/Kids [3 0 R 6 0 R] /Count 2"), "{text}");
    assert!(text.contains("/MediaBox [0 0 30 10]"), "{text}");
    assert!(text.contains("/MediaBox [0 0 10 30]"), "{text}");
    // Each cross-reference offset points at its object.
    let xref = text.rfind("xref\n").unwrap();
    for (n, line) in text[xref..].lines().skip(3).take(8).enumerate() {
        let offset: usize = line[..10].parse().unwrap();
        assert!(text[offset..].starts_with(&format!("{} 0 obj", n + 1)));
    }
}

#[test]
fn formats_are_known_by_content_and_extension() {
    assert_eq!(
        ImageFormat::from_bytes(&jpeg(1, 1, [0; 3])),
        Some(ImageFormat::Jpeg)
    );
    assert_eq!(
        ImageFormat::from_bytes(&webp_still([0; 4])),
        Some(ImageFormat::WebP)
    );
    assert_eq!(
        ImageFormat::from_bytes(&gif(&[[0; 4]])),
        Some(ImageFormat::Gif)
    );
    assert_eq!(ImageFormat::from_bytes(b"%PDF-1.7"), None);
    for (extension, format) in [
        ("JPG", ImageFormat::Jpeg),
        ("jpeg", ImageFormat::Jpeg),
        ("png", ImageFormat::Png),
        ("WebP", ImageFormat::WebP),
        ("gif", ImageFormat::Gif),
    ] {
        assert_eq!(ImageFormat::from_extension(extension), Some(format));
    }
    assert_eq!(ImageFormat::from_extension("tif"), Some(ImageFormat::Tiff));
    for magic in [b"II*\0".as_slice(), b"MM\0*", b"II+\0", b"MM\0+"] {
        assert_eq!(ImageFormat::from_bytes(magic), Some(ImageFormat::Tiff));
    }
    assert_eq!(ImageFormat::from_extension("heic"), None);
}

/// The test images the import-seam tests share, written to `dir`.
pub(crate) fn write_photo(dir: &Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    let photo = with_segments(&jpeg(64, 48, [90, 160, 60]), &[app1_exif(Some(6))]);
    std::fs::write(&path, photo).unwrap();
    path
}

pub(crate) fn bomb() -> Vec<u8> {
    png_claiming(100_000, 100_000)
}

/// Writes `tests/fixtures/images/gps-rotated.jpg` (a 64×48 green photo
/// with a fictional camera make, a GPS position and orientation 6), which
/// the phones' intake test reads. Run by hand.
#[test]
#[ignore = "writes a committed fixture"]
fn write_image_fixtures() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/images");
    std::fs::create_dir_all(&dir).unwrap();
    write_photo(&dir, "gps-rotated.jpg");
}

/// At the import seam: an image file in, an import or its error out.
mod import_seam {
    use super::{ImageError, bomb, write_photo};
    use crate::import::{
        ImportError, LeftOut, image_warning, prepare, supported_extension, supported_source,
    };
    use std::path::Path;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// A photo imports as one scanned page: without OCR installed the
    /// import asks for it, naming the photo; without OCR at all there is
    /// no text. Nothing is written beside the photo.
    #[test]
    fn a_photo_imports_as_one_scanned_page() {
        let dir = tempfile::tempdir().unwrap();
        let photo = write_photo(dir.path(), "Lease photo.jpg");
        match prepare(&photo, None, false) {
            Err(ImportError::NeedsOcr(path, pages, _, page_count)) => {
                assert_eq!(path, std::path::absolute(&photo).unwrap());
                assert_eq!((pages, page_count), (vec![1], 1));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            prepare(&photo, None, true),
            Err(ImportError::NoNativeText)
        ));
        assert_eq!(names(dir.path()), ["Lease photo.jpg"]);
    }

    /// Images are admitted by extension, and by content when renamed.
    #[test]
    fn images_are_admitted_by_extension_and_content() {
        let dir = tempfile::tempdir().unwrap();
        let photo = write_photo(dir.path(), "photo.jpg");
        let renamed = dir.path().join("photo.bin");
        std::fs::copy(&photo, &renamed).unwrap();
        assert!(supported_source(&renamed));
        assert!(matches!(
            prepare(&renamed, None, false),
            Err(ImportError::NeedsOcr(..))
        ));
        for name in ["a.jpg", "a.JPEG", "a.png", "a.webp", "a.gif"] {
            assert!(supported_extension(Path::new(name)), "{name}");
        }
        // HEIC comes with #1024 item 9.
        assert!(!supported_extension(Path::new("a.heic")));
    }

    #[test]
    fn too_large_and_damaged_images_say_so() {
        let dir = tempfile::tempdir().unwrap();
        let large = dir.path().join("huge.png");
        std::fs::write(&large, bomb()).unwrap();
        let error = prepare(&large, None, false).unwrap_err();
        assert!(
            matches!(error, ImportError::Image(ImageError::TooLarge)),
            "{error:?}"
        );
        assert_eq!(error.to_string(), "This image is too large to import.");
        let damaged = dir.path().join("damaged.png");
        std::fs::write(&damaged, b"\x89PNG\r\n\x1a\n not an image").unwrap();
        let error = prepare(&damaged, None, false).unwrap_err();
        assert!(
            matches!(error, ImportError::Image(ImageError::Unreadable)),
            "{error:?}"
        );
    }

    /// The left-out line counts an animation's later frames.
    #[test]
    fn the_left_out_line_counts_animation_frames() {
        let frames = |count| {
            LeftOut {
                animation_frames: count,
                ..LeftOut::default()
            }
            .line(true)
        };
        assert_eq!(frames(0), None);
        assert_eq!(
            frames(1).as_deref(),
            Some("1 animation frame not imported.")
        );
        assert_eq!(
            frames(29).as_deref(),
            Some("29 animation frames not imported.")
        );
    }

    /// An image's OCR warning sends the reader to the image.
    #[test]
    fn an_image_warning_names_the_image() {
        assert_eq!(
            image_warning(Some(
                "OCR review needed: pages 1 of 1 may be incomplete or inaccurate. Compare with the source PDF."
                    .into()
            ))
            .as_deref(),
            Some(
                "OCR review needed: pages 1 of 1 may be incomplete or inaccurate. Compare with the source image."
            )
        );
    }

    const CHILD_PHOTO: &str = "MDOC_TEST_IMAGE_IMPORT_PHOTO";

    /// Nothing decoded or rendered reaches disk: an image's import, run in a
    /// child process whose temporary folder is an empty one, leaves that
    /// folder empty.
    #[test]
    fn an_image_import_writes_nothing_to_the_temporary_folder() {
        let dir = tempfile::tempdir().unwrap();
        let temp = dir.path().join("temp");
        std::fs::create_dir(&temp).unwrap();
        let photo = write_photo(dir.path(), "photo.jpg");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "image_pdf::tests::import_seam::child_imports_a_photo",
                "--include-ignored",
            ])
            .env(CHILD_PHOTO, &photo)
            .env("TMP", &temp)
            .env("TEMP", &temp)
            .env("TMPDIR", &temp)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{stdout}");
        assert!(stdout.contains("1 passed"), "{stdout}");
        assert!(names(&temp).is_empty(), "{:?}", names(&temp));
    }

    /// The child of the test above.
    #[test]
    #[ignore = "run by an_image_import_writes_nothing_to_the_temporary_folder"]
    fn child_imports_a_photo() {
        let photo = std::env::var_os(CHILD_PHOTO).expect("run by its parent test");
        let temp = std::env::var_os("TMP")
            .or_else(|| std::env::var_os("TMPDIR"))
            .unwrap();
        assert_eq!(
            std::fs::canonicalize(std::env::temp_dir()).unwrap(),
            std::fs::canonicalize(temp).unwrap()
        );
        let photo = Path::new(&photo);
        assert!(matches!(
            prepare(photo, None, false),
            Err(ImportError::NeedsOcr(..))
        ));
        let built = super::build_one(&std::fs::read(photo).unwrap());
        assert_eq!(built.pages, 1);
    }
}

/// A JPEG missing a table its scan uses can't be decoded, so it doesn't
/// pass through as a page OCR would read broken: the decoder refuses it,
/// at the builder and at the import.
#[test]
fn a_jpeg_missing_its_tables_is_refused() {
    let clean = jpeg(64, 48, [90, 160, 60]);
    for marker in [0xdb, 0xc4] {
        // The file without its quantization (DB) or Huffman (C4) tables.
        let mut broken = clean[..2].to_vec();
        let mut at = 2;
        while at + 4 <= clean.len() && clean[at] == 0xff {
            let kind = clean[at + 1];
            let length = usize::from(u16::from_be_bytes([clean[at + 2], clean[at + 3]]));
            if kind != marker {
                broken.extend_from_slice(&clean[at..at + 2 + length]);
            }
            at += 2 + length;
            if kind == 0xda {
                broken.extend_from_slice(&clean[at..]);
                break;
            }
        }
        assert!(
            matches!(build(&[&broken], DEFAULT_DPI), Err(ImageError::Unreadable)),
            "without {marker:02x}"
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.jpg");
        std::fs::write(&path, &broken).unwrap();
        assert!(matches!(
            crate::import::prepare(&path, None, false),
            Err(crate::import::ImportError::Image(ImageError::Unreadable))
        ));
    }
}

/// Of APP14, only Adobe's colour-transform marker travels, rebuilt from its
/// fixed fields: an application's own APP14 data doesn't.
#[test]
fn only_adobe_s_colour_marker_survives_from_app14() {
    let clean = jpeg(64, 48, [90, 160, 60]);
    let adobe = b"Adobe\x00\x64\x00\x00\x00\x00\x01";
    let photo = with_segments(
        &clean,
        &[
            segment(0xee, &[adobe.as_slice(), b"PRIVATE-TRAILER"].concat()),
            segment(0xee, b"VendorPrivate fictional data"),
        ],
    );
    let built = build_one(&photo);
    let page = page(&built.pdf);
    assert_eq!(
        page.data,
        with_segments(&without_app0(&clean), &[segment(0xee, adobe)])
    );
    for leaked in [b"PRIVATE-TRAILER".as_slice(), b"VendorPrivate"] {
        assert!(
            !contains(&built.pdf, leaked),
            "{}",
            String::from_utf8_lossy(leaked)
        );
    }
}

/// A JPEG whose comment holds `%PDF-` is still a JPEG: its own signature
/// comes first, at the import as at the Original.
#[test]
fn an_image_mentioning_pdf_is_still_an_image() {
    let photo = with_segments(
        &jpeg(64, 48, [90, 160, 60]),
        &[segment(0xfe, b"%PDF-1.7 fictional comment")],
    );
    assert!(!crate::import::is_pdf(&photo));
    let built = through_import("mention.jpg", &photo);
    assert!(!contains(&built.pdf, b"fictional comment"));
}

/// An animated image's decoder holds a canvas and a frame besides its
/// output: a WebP whose canvas fits the budget once but not three times is
/// refused before decoding.
#[test]
fn a_large_animation_is_refused_before_decoding() {
    let mut animation = webp_animated(&[[200, 0, 0, 255], [0, 200, 0, 255]]);
    // The VP8X canvas, 11,000 by 11,000: 484 MB of RGBA once.
    let canvas = 12 + 8 + 4;
    let side = (11_000u32 - 1).to_le_bytes();
    animation[canvas..canvas + 3].copy_from_slice(&side[..3]);
    animation[canvas + 3..canvas + 6].copy_from_slice(&side[..3]);
    assert!(matches!(
        build(&[&animation], DEFAULT_DPI),
        Err(ImageError::TooLarge)
    ));
}
