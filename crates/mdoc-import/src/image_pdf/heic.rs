//! HEIC and HEIF (#1024 decision 13): the operating system decodes them
//! (ImageIO on macOS, WIC with Microsoft's HEIF and HEVC extensions on
//! Windows), and hands the builder 8-bit RGB or RGBA pixels and their EXIF
//! orientation. The builder turns the pixels upright, puts any
//! transparency on white and stores them as a quality-92 JPEG, upright
//! itself, so none of the file's metadata (EXIF, GPS, XMP, camera, dates)
//! reaches the PDF. Nothing is written to disk.
//!
//! [`OsDecoder`] is the seam: tests put a stub in place of the platform's
//! decoder ([`with_decoder`]), so CI needs no codec.

use super::{ImageError, MAX_ALLOC, MAX_DIMENSION};

#[cfg(test)]
mod tests;

/// What an operating system's decoder gives: the pixels as stored, 8-bit
/// RGB or RGBA (straight alpha), and the EXIF orientation that shows them
/// upright (1 when the decoder already turned them).
pub struct OsDecoded {
    pub image: image::DynamicImage,
    pub orientation: u8,
}

/// Decodes the formats whose codec only the operating system carries.
pub trait OsDecoder {
    fn decode_heic(&self, bytes: &[u8]) -> Result<OsDecoded, ImageError>;
}

/// The extensions a Windows PC needs to decode HEIC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingCodec {
    /// Microsoft's HEIF Image Extensions: without them WIC has no HEIF
    /// decoder at all, so whether HEVC is there can't be told.
    HeifExtension,
    /// Microsoft's HEVC Video Extensions: the HEIF decoder is there, but it
    /// can't decode the photo's HEVC data.
    HevcExtension,
}

/// The most a HEIC file may hold: it goes whole to the operating system's
/// decoder (WIC's stream length is 32-bit), and no photo is this large.
/// The PDF import's cap.
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// The HEVC-coded HEIF brands.
const HEVC_BRANDS: [&[u8; 4]; 8] = [
    b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"hevm", b"hevs",
];

/// The brands an `ftyp` box at the start of `bytes` names: its major
/// brand, then its compatible brands. `None` when `bytes` don't start with
/// one.
fn brands(bytes: &[u8]) -> Option<impl Iterator<Item = &[u8; 4]> + Clone> {
    if bytes.len() < 16 || &bytes[4..8] != b"ftyp" {
        return None;
    }
    let size = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let end = size.clamp(16, bytes.len().min(256));
    let major: &[u8; 4] = bytes[8..12].try_into().expect("four bytes");
    // The minor version sits between the major and the compatible brands.
    Some(std::iter::once(major).chain(bytes[16..end].as_chunks::<4>().0))
}

/// Whether `bytes` start as a HEIF file that may be a HEIC: an `ftyp` box
/// naming an HEVC brand or a generic HEIF one (`mif1`, `msf1`), whose
/// `meta` box may lie further on. The import's read stays bounded for
/// these.
pub(crate) fn is_heif_container(bytes: &[u8]) -> bool {
    brands(bytes).is_some_and(|mut brands| {
        brands.any(|brand| HEVC_BRANDS.contains(&brand) || brand == b"mif1" || brand == b"msf1")
    })
}

/// Whether `bytes` are a HEIC or HEIF image coded with HEVC: an ISO base
/// media file whose `ftyp` box names an HEVC-coded HEIF brand, or names
/// only the generic HEIF brands (`mif1`, `msf1`) and whose primary item is
/// HEVC-coded. AVIF, also HEIF but AV1-coded, is not.
pub(super) fn is_heic(bytes: &[u8]) -> bool {
    let Some(mut brands) = brands(bytes) else {
        return false;
    };
    if brands.clone().any(|brand| HEVC_BRANDS.contains(&brand)) {
        return true;
    }
    // A generic brand leaves the codec to the primary item.
    brands.any(|brand| brand == b"mif1" || brand == b"msf1")
        && primary_item_type(bytes).is_some_and(|kind| kind == *b"hvc1" || kind == *b"grid")
        && !items_of_type(bytes, *b"av01")
}

/// The largest `meta` box the file probe reads: a photo's is a few
/// kilobytes.
const MAX_META_BYTES: u64 = 16 * 1024 * 1024;

/// The top-level boxes the file probe walks past before it gives up.
const MAX_PROBED_BOXES: usize = 1024;

/// The format of the HEIF `file` whose first bytes, `head`, didn't settle
/// it: its top-level boxes are walked by their headers alone to its `meta`
/// box, which is read on its own and judged with the `ftyp` box, so a
/// `meta` box after a large `mdat` is found without reading the photo.
pub(super) fn probe_file(
    file: &mut (impl std::io::Read + std::io::Seek),
    head: &[u8],
) -> std::io::Result<bool> {
    use std::io::{Read, SeekFrom};
    let ftyp = u64::from(u32::from_be_bytes(
        head[..4].try_into().expect("four bytes"),
    ));
    let Some(ftyp) = usize::try_from(ftyp).ok().and_then(|size| head.get(..size)) else {
        return Ok(false);
    };
    let length = file.seek(SeekFrom::End(0))?;
    let mut at = ftyp.len() as u64;
    for _ in 0..MAX_PROBED_BOXES {
        if at.checked_add(8).is_none_or(|end| end > length) {
            break;
        }
        file.seek(SeekFrom::Start(at))?;
        let mut header = [0u8; 16];
        file.read_exact(&mut header[..8])?;
        let size = u64::from(u32::from_be_bytes(
            header[..4].try_into().expect("four bytes"),
        ));
        let size = match size {
            0 => length - at,
            1 => {
                if at + 16 > length {
                    break;
                }
                file.read_exact(&mut header[8..])?;
                u64::from_be_bytes(header[8..].try_into().expect("eight bytes"))
            }
            size => size,
        };
        if size < 8 || at.checked_add(size).is_none_or(|end| end > length) {
            break;
        }
        if &header[4..8] == b"meta" {
            if size > MAX_META_BYTES {
                break;
            }
            let mut probe = ftyp.to_vec();
            file.seek(SeekFrom::Start(at))?;
            (&mut *file).take(size).read_to_end(&mut probe)?;
            return Ok(is_heic(&probe));
        }
        at += size;
    }
    Ok(false)
}

/// The boxes in `data`, as (type, body), in order; a box that runs past the
/// end ends the list.
fn boxes(data: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    let mut at = 0usize;
    std::iter::from_fn(move || {
        let header = data.get(at..at.checked_add(8)?)?;
        let size = u32::from_be_bytes(header[..4].try_into().expect("four bytes")) as u64;
        let kind: [u8; 4] = header[4..8].try_into().expect("four bytes");
        // A malformed size (past the end, or overflowing) ends the list.
        let (body_at, end) = match size {
            // A 64-bit size follows the type.
            1 => {
                let large = data.get(at + 8..at.checked_add(16)?)?;
                let size = u64::from_be_bytes(large.try_into().expect("eight bytes"));
                (at + 16, (at as u64).checked_add(size)?)
            }
            // To the end of the data.
            0 => (at + 8, data.len() as u64),
            _ => (at + 8, (at as u64).checked_add(size)?),
        };
        let end = usize::try_from(end).ok()?;
        let body = data.get(body_at..end)?;
        at = end;
        Some((kind, body))
    })
}

/// The body of the top-level `meta` box, past its version and flags.
fn meta(bytes: &[u8]) -> Option<&[u8]> {
    boxes(bytes)
        .find(|(kind, _)| kind == b"meta")
        .and_then(|(_, body)| body.get(4..))
}

/// The item types the `meta` box's `iinf` lists, with their ids.
fn item_types(meta: &[u8]) -> Vec<(u32, [u8; 4])> {
    let Some((_, iinf)) = boxes(meta).find(|(kind, _)| kind == b"iinf") else {
        return Vec::new();
    };
    let version = iinf.first().copied().unwrap_or(0);
    let entries = if version == 0 { 6 } else { 8 };
    boxes(iinf.get(entries..).unwrap_or_default())
        .filter(|(kind, _)| kind == b"infe")
        .filter_map(|(_, infe)| {
            // Versions 2 and 3 name the item type; earlier ones can't be
            // image items.
            let (id, rest) = match infe.first()? {
                2 => (
                    u32::from(u16::from_be_bytes(infe.get(4..6)?.try_into().ok()?)),
                    6,
                ),
                3 => (u32::from_be_bytes(infe.get(4..8)?.try_into().ok()?), 8),
                _ => return None,
            };
            // The protection index, then the type.
            Some((id, infe.get(rest + 2..rest + 6)?.try_into().ok()?))
        })
        .collect()
}

/// The type of the item the `pitm` box names primary.
fn primary_item_type(bytes: &[u8]) -> Option<[u8; 4]> {
    let meta = meta(bytes)?;
    let id = primary_item(meta)?;
    item_types(meta)
        .into_iter()
        .find(|(item, _)| *item == id)
        .map(|(_, kind)| kind)
}

/// Whether any item in the `meta` box is of `kind`.
fn items_of_type(bytes: &[u8], kind: [u8; 4]) -> bool {
    meta(bytes).is_some_and(|meta| item_types(meta).iter().any(|(_, item)| *item == kind))
}

/// The id of the primary item, from the `pitm` box.
fn primary_item(meta: &[u8]) -> Option<u32> {
    let (_, pitm) = boxes(meta).find(|(kind, _)| kind == b"pitm")?;
    match pitm.first()? {
        0 => Some(u32::from(u16::from_be_bytes(
            pitm.get(4..6)?.try_into().ok()?,
        ))),
        _ => Some(u32::from_be_bytes(pitm.get(4..8)?.try_into().ok()?)),
    }
}

/// The auxiliary item that holds the primary image's alpha plane: an item
/// that refers to the primary as its auxiliary (`iref`'s `auxl`) and whose
/// `auxC` property names alpha (not depth, which phones store the same
/// way). For a decoder that drops the alpha plane (Windows' HEIF
/// extension), the builder decodes this item on its own.
#[cfg(any(target_os = "windows", test))]
pub(super) fn alpha_item(bytes: &[u8]) -> Option<u32> {
    const ALPHA: [&[u8]; 2] = [
        b"urn:mpeg:hevc:2015:auxid:1",
        b"urn:mpeg:mpegB:cicp:systems:auxiliary:alpha",
    ];
    let meta = meta(bytes)?;
    let primary = primary_item(meta)?;
    let (_, iref) = boxes(meta).find(|(kind, _)| kind == b"iref")?;
    let wide = *iref.first()? != 0;
    let id = |data: &[u8], at: usize| -> Option<(u32, usize)> {
        if wide {
            Some((
                u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?),
                at + 4,
            ))
        } else {
            Some((
                u32::from(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?)),
                at + 2,
            ))
        }
    };
    let auxiliaries: Vec<u32> = boxes(iref.get(4..)?)
        .filter(|(kind, _)| kind == b"auxl")
        .filter_map(|(_, reference)| {
            let (from, at) = id(reference, 0)?;
            let count = u16::from_be_bytes(reference.get(at..at + 2)?.try_into().ok()?);
            let mut at = at + 2;
            for _ in 0..count {
                let (to, next) = id(reference, at)?;
                if to == primary {
                    return Some(from);
                }
                at = next;
            }
            None
        })
        .collect();
    let (_, iprp) = boxes(meta).find(|(kind, _)| kind == b"iprp")?;
    let (_, ipco) = boxes(iprp).find(|(kind, _)| kind == b"ipco")?;
    let properties: Vec<([u8; 4], &[u8])> = boxes(ipco).collect();
    // Associations may be split across several `ipma` boxes (one per
    // version and flags pair): every one counts.
    boxes(iprp)
        .filter(|(kind, _)| kind == b"ipma")
        .filter_map(|(_, ipma)| associations(ipma))
        .flatten()
        .find(|(item, indices)| {
            auxiliaries.contains(item)
                && indices.iter().any(|&index| {
                    properties
                        .get(index.wrapping_sub(1))
                        .is_some_and(|(kind, body)| {
                            // A full box: version and flags, then the type
                            // as a string ending in a zero.
                            kind == b"auxC"
                                && body.get(4..).is_some_and(|name| {
                                    let end = name
                                        .iter()
                                        .position(|&byte| byte == 0)
                                        .unwrap_or(name.len());
                                    ALPHA.contains(&&name[..end])
                                })
                        })
                })
        })
        .map(|(item, _)| item)
}

/// An `ipma` box's entries: each item and the 1-based `ipco` indices of
/// its properties. `None` for a box that runs short.
#[cfg(any(target_os = "windows", test))]
fn associations(ipma: &[u8]) -> Option<Vec<(u32, Vec<usize>)>> {
    let (version, flags) = (*ipma.first()?, *ipma.get(3)?);
    let count = u32::from_be_bytes(ipma.get(4..8)?.try_into().ok()?);
    let mut at = 8;
    let mut entries = Vec::new();
    for _ in 0..count {
        let (item, next) = if version < 1 {
            (
                u32::from(u16::from_be_bytes(ipma.get(at..at + 2)?.try_into().ok()?)),
                at + 2,
            )
        } else {
            (
                u32::from_be_bytes(ipma.get(at..at + 4)?.try_into().ok()?),
                at + 4,
            )
        };
        let count = usize::from(*ipma.get(next)?);
        at = next + 1;
        let mut indices = Vec::with_capacity(count);
        for _ in 0..count {
            // The essential bit, then the index.
            if flags & 1 != 0 {
                indices.push(usize::from(
                    u16::from_be_bytes(ipma.get(at..at + 2)?.try_into().ok()?) & 0x7fff,
                ));
                at += 2;
            } else {
                indices.push(usize::from(*ipma.get(at)? & 0x7f));
                at += 1;
            }
        }
        entries.push((item, indices));
    }
    Some(entries)
}

/// `bytes` with `item` named primary instead, the file otherwise as it is:
/// a copy whose decode is that item's image. `None` when the `pitm` box
/// can't name it in place.
#[cfg(any(target_os = "windows", test))]
pub(super) fn with_primary(bytes: &[u8], item: u32) -> Option<Vec<u8>> {
    let meta = meta(bytes)?;
    let (_, pitm) = boxes(meta).find(|(kind, _)| kind == b"pitm")?;
    // Where the `pitm` body sits in `bytes`: both are slices of it.
    let at = pitm.as_ptr() as usize - bytes.as_ptr() as usize;
    let mut copy = bytes.to_vec();
    match pitm.first()? {
        0 => copy
            .get_mut(at + 4..at + 6)?
            .copy_from_slice(&u16::try_from(item).ok()?.to_be_bytes()),
        _ => copy
            .get_mut(at + 4..at + 8)?
            .copy_from_slice(&item.to_be_bytes()),
    }
    Some(copy)
}

/// Whether an image `width` by `height` fits the builder's limits: at most
/// [`MAX_DIMENSION`] pixels a side and [`MAX_ALLOC`] as RGBA. Each decoder
/// asks with the stored size before it decodes, and the page asks again.
pub(super) fn fits(width: u32, height: u32) -> Result<(), ImageError> {
    if width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) * 4 > MAX_ALLOC
    {
        return Err(ImageError::TooLarge);
    }
    Ok(())
}

/// `bytes` decoded by the platform's decoder (or the test's stub), as a
/// page by the builder's rules for a photo: turned upright in its pixels
/// (decision 13: the JPEG itself is upright), on white.
pub(super) fn page(bytes: &[u8]) -> Result<super::Page, ImageError> {
    // Whatever read the file, no more than this goes to the decoder.
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(ImageError::TooLarge);
    }
    let decoded = decode(bytes)?;
    let mut image = decoded.image;
    fits(image.width(), image.height())?;
    if let Some(orientation) = image::metadata::Orientation::from_exif(decoded.orientation) {
        image.apply_orientation(orientation);
    }
    super::encode(image, 1, true)
}

fn decode(bytes: &[u8]) -> Result<OsDecoded, ImageError> {
    #[cfg(test)]
    if let Some(result) =
        STUB.with(|stub| stub.borrow().as_ref().map(|stub| stub.decode_heic(bytes)))
    {
        return result;
    }
    platform::Decoder.decode_heic(bytes)
}

#[cfg(test)]
thread_local! {
    static STUB: std::cell::RefCell<Option<Box<dyn OsDecoder>>> = const { std::cell::RefCell::new(None) };
}

/// Run `run` with `decoder` in place of the platform's, on this thread.
#[cfg(test)]
pub(crate) fn with_decoder<T>(decoder: impl OsDecoder + 'static, run: impl FnOnce() -> T) -> T {
    STUB.with(|stub| *stub.borrow_mut() = Some(Box::new(decoder)));
    let result = run();
    STUB.with(|stub| *stub.borrow_mut() = None);
    result
}

#[cfg(target_os = "windows")]
mod wic;
#[cfg(target_os = "windows")]
use wic as platform;

#[cfg(target_os = "macos")]
mod image_io;
#[cfg(target_os = "macos")]
use image_io as platform;

/// Elsewhere (Linux, and the phones, whose apps decode HEIC themselves)
/// there is no decoder here.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod platform {
    use super::{ImageError, OsDecoded, OsDecoder};

    pub(super) struct Decoder;

    impl OsDecoder for Decoder {
        fn decode_heic(&self, _: &[u8]) -> Result<OsDecoded, ImageError> {
            Err(ImageError::HeicUnsupported)
        }
    }
}
