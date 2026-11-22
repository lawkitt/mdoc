//! An image file without its metadata, for the copy a phone keeps as the
//! Original (#1021): the same pixels, byte for byte, with nothing a camera,
//! editor or phone wrote about them. Only the EXIF orientation stays, as
//! the one tag of a minimal EXIF block, so a viewer and the image builder
//! still draw the image upright.

use super::{ImageFormat, jpeg};

/// `bytes` without metadata, in their own format: EXIF (GPS, camera,
/// dates), XMP, ICC profiles, text chunks, comments and appended data go;
/// the orientation stays. `None` for a format or a file this can't rewrite
/// (HEIC, TIFF, a JPEG the passthrough doesn't parse): a phone has its
/// operating system draw a HEIC as an upright JPEG, and refuses a TIFF
/// (TIFF on the phones is #1064).
pub fn without_metadata(bytes: &[u8]) -> Option<Vec<u8>> {
    match ImageFormat::from_bytes(bytes)? {
        ImageFormat::Jpeg => jpeg_without_metadata(bytes),
        ImageFormat::Png => png_without_metadata(bytes),
        ImageFormat::WebP => webp_without_metadata(bytes),
        ImageFormat::Gif => gif_without_metadata(bytes),
        ImageFormat::Tiff | ImageFormat::Heic => None,
    }
}

/// A big-endian TIFF block holding only `orientation` (tag 0x0112), as
/// EXIF carries it.
fn orientation_tiff(orientation: u8) -> Vec<u8> {
    let mut tiff = b"MM\x00\x2a\x00\x00\x00\x08\x00\x01".to_vec();
    tiff.extend_from_slice(&[0x01, 0x12, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01]);
    tiff.extend_from_slice(&[0x00, orientation, 0x00, 0x00]);
    tiff.extend_from_slice(&[0x00; 4]);
    tiff
}

fn jpeg_without_metadata(bytes: &[u8]) -> Option<Vec<u8>> {
    let (kept, orientation) = jpeg::without_metadata(bytes)?;
    if orientation == 1 {
        return Some(kept);
    }
    let exif = [b"Exif\x00\x00".as_slice(), &orientation_tiff(orientation)].concat();
    let mut out = Vec::with_capacity(kept.len() + exif.len() + 4);
    out.extend_from_slice(&kept[..2]);
    out.extend_from_slice(&[0xff, 0xe1]);
    out.extend_from_slice(&u16::try_from(exif.len() + 2).ok()?.to_be_bytes());
    out.extend_from_slice(&exif);
    out.extend_from_slice(&kept[2..]);
    Some(out)
}

/// The chunks a PNG keeps: the image itself, its transparency and how its
/// colours are meant (no profile, which may carry a name).
const PNG_KEPT: [&[u8; 4]; 7] = [
    b"IHDR", b"PLTE", b"IDAT", b"IEND", b"tRNS", b"gAMA", b"sRGB",
];

fn png_without_metadata(bytes: &[u8]) -> Option<Vec<u8>> {
    const SIGNATURE: usize = 8;
    let mut out = bytes.get(..SIGNATURE)?.to_vec();
    let mut orientation = 1;
    let mut at = SIGNATURE;
    let mut ended = false;
    while !ended {
        let length =
            usize::try_from(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?)).ok()?;
        let end = at.checked_add(12)?.checked_add(length)?;
        let chunk = bytes.get(at..end)?;
        let kind: &[u8; 4] = chunk[4..8].try_into().ok()?;
        if kind == b"eXIf" {
            orientation = exif_orientation(&chunk[8..8 + length]);
        }
        // The orientation goes before the image data, where readers look.
        if kind == b"IDAT" && orientation != 1 {
            out.extend(png_chunk(b"eXIf", &orientation_tiff(orientation)));
            orientation = 1;
        }
        if PNG_KEPT.contains(&kind) {
            out.extend_from_slice(chunk);
        }
        ended = kind == b"IEND";
        at = end;
    }
    Some(out)
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = u32::try_from(data.len())
        .expect("a TIFF block is small")
        .to_be_bytes()
        .to_vec();
    chunk.extend_from_slice(kind);
    chunk.extend_from_slice(data);
    let mut crc = flate2::Crc::new();
    crc.update(kind);
    crc.update(data);
    chunk.extend_from_slice(&crc.sum().to_be_bytes());
    chunk
}

/// The orientation an EXIF TIFF block holds, 1 without one.
fn exif_orientation(tiff: &[u8]) -> u8 {
    let tiff = tiff.strip_prefix(b"Exif\x00\x00").unwrap_or(tiff);
    image::metadata::Orientation::from_exif_chunk(tiff).map_or(1, |o| o.to_exif())
}

/// The `VP8X` flag for EXIF.
const WEBP_EXIF: u8 = 0x08;

fn webp_without_metadata(bytes: &[u8]) -> Option<Vec<u8>> {
    let length = usize::try_from(u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?)).ok()?;
    let mut chunks = bytes.get(12..8usize.checked_add(length)?)?;
    let mut out = bytes[..12].to_vec();
    let mut orientation = 1;
    let mut flags_at = None;
    while !chunks.is_empty() {
        let (kind, data) = take_webp_chunk(&mut chunks)?;
        match kind {
            b"EXIF" => orientation = exif_orientation(data),
            b"VP8X" => {
                let mut header: [u8; 10] = data.try_into().ok()?;
                header[0] &= 0x12; // Alpha and animation.
                header[1..4].fill(0);
                flags_at = Some(out.len() + 8);
                write_webp_chunk(&mut out, kind, &header)?;
            }
            b"ANIM" if data.len() == 6 => write_webp_chunk(&mut out, kind, data)?,
            b"ALPH" | b"VP8 " | b"VP8L" => write_webp_chunk(&mut out, kind, data)?,
            b"ANMF" => {
                let mut frame = data.get(..16)?.to_vec();
                frame[15] &= 0x03; // Blending and disposal.
                let mut subchunks = &data[16..];
                while !subchunks.is_empty() {
                    let (kind, data) = take_webp_chunk(&mut subchunks)?;
                    if matches!(kind, b"ALPH" | b"VP8 " | b"VP8L") {
                        write_webp_chunk(&mut frame, kind, data)?;
                    }
                }
                write_webp_chunk(&mut out, kind, &frame)?;
            }
            _ => {}
        }
    }
    if orientation != 1 {
        // A simple WebP can't announce EXIF. Let the operating system apply it.
        out[flags_at?] |= WEBP_EXIF;
        write_webp_chunk(&mut out, b"EXIF", &orientation_tiff(orientation))?;
    }
    let length = u32::try_from(out.len() - 8).ok()?;
    out[4..8].copy_from_slice(&length.to_le_bytes());
    Some(out)
}

fn take_webp_chunk<'a>(chunks: &mut &'a [u8]) -> Option<(&'a [u8; 4], &'a [u8])> {
    let (header, body) = chunks.split_at_checked(8)?;
    let size = usize::try_from(u32::from_le_bytes(header[4..].try_into().ok()?)).ok()?;
    let data = body.get(..size)?;
    *chunks = body.get(size.checked_add(size & 1)?..)?;
    Some((header[..4].try_into().ok()?, data))
}

fn write_webp_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) -> Option<()> {
    out.extend_from_slice(kind);
    out.extend_from_slice(&u32::try_from(data.len()).ok()?.to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
    Some(())
}

/// The application extensions a GIF keeps: looping, which says how it
/// plays. Others (XMP, ICC profiles) are metadata.
const GIF_KEPT_APPLICATIONS: [&[u8; 11]; 2] = [b"NETSCAPE2.0", b"ANIMEXTS1.0"];

fn gif_without_metadata(bytes: &[u8]) -> Option<Vec<u8>> {
    // Header and logical screen descriptor, then the global colour table.
    let flags = *bytes.get(10)?;
    let mut at = 13 + super::color_table(flags);
    let mut out = bytes.get(..at)?.to_vec();
    // Data sub-blocks from `at`, through their terminator.
    let blocks = |mut at: usize| -> Option<usize> {
        loop {
            let length = usize::from(*bytes.get(at)?);
            at += 1 + length;
            if length == 0 {
                return Some(at);
            }
        }
    };
    loop {
        match *bytes.get(at)? {
            0x21 => {
                let label = *bytes.get(at + 1)?;
                let end = blocks(at + 2)?;
                let keep = match label {
                    // Comments.
                    0xfe => false,
                    0xff => bytes
                        .get(at + 3..at + 14)
                        .is_some_and(|name| GIF_KEPT_APPLICATIONS.iter().any(|kept| *kept == name)),
                    // Graphic control and plain text belong to the image.
                    _ => true,
                };
                if keep {
                    out.extend_from_slice(&bytes[at..end]);
                }
                at = end;
            }
            0x2c => {
                let flags = *bytes.get(at + 9)?;
                let end = blocks(at + 10 + super::color_table(flags) + 1)?;
                out.extend_from_slice(bytes.get(at..end)?);
                at = end;
            }
            0x3b => {
                out.push(0x3b);
                return Some(out);
            }
            _ => return None,
        }
    }
}
