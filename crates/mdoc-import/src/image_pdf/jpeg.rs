//! JPEG passthrough: the file's own compressed data, without its metadata.

use super::{MAX_LONG_SIDE, Page, Stream};

/// `jpeg` as a page without re-encoding, when it can pass through: Huffman
/// coded (baseline, extended or progressive), 8-bit, one or three
/// components, its long side at most [`MAX_LONG_SIDE`]. `None` otherwise,
/// and for a file that doesn't parse, so the decoder has its say.
///
/// The page keeps every segment a decoder needs and loses the rest:
/// APP0–APP13 and APP15 (JFIF, EXIF with its GPS position and camera,
/// XMP, ICC profiles, MPF and its previews, Photoshop's resources) and
/// comments go. APP14 stays: Adobe's colour transform flag holds no
/// metadata and tells the decoder how to read the colours. Anything after
/// the end-of-image marker (depth maps, appended previews) goes too.
pub(super) fn passthrough(jpeg: &[u8]) -> Option<Page> {
    let segments = Segments::read(jpeg)?;
    let frame = segments.frame?;
    if frame.precision != 8
        || !matches!(frame.components, 1 | 3)
        || frame.width == 0
        || frame.height == 0
        || u32::from(frame.width.max(frame.height)) > MAX_LONG_SIDE
    {
        return None;
    }
    Some(Page {
        width: u32::from(frame.width),
        height: u32::from(frame.height),
        gray: frame.components == 1,
        stream: Stream::Jpeg,
        data: segments.kept,
        orientation: segments.orientation,
    })
}

/// `jpeg` without the segments [`passthrough`] drops, whatever its size,
/// and its EXIF orientation; `None` for a file that doesn't parse.
pub(super) fn without_metadata(jpeg: &[u8]) -> Option<(Vec<u8>, u8)> {
    let segments = Segments::read(jpeg)?;
    Some((segments.kept, segments.orientation))
}

struct Frame {
    precision: u8,
    height: u16,
    width: u16,
    components: u8,
}

/// A JPEG read segment by segment.
struct Segments {
    /// The file without its metadata segments.
    kept: Vec<u8>,
    frame: Option<Frame>,
    /// The first EXIF block's orientation, 1 when it has none.
    orientation: u8,
}

impl Segments {
    fn read(jpeg: &[u8]) -> Option<Self> {
        if !jpeg.starts_with(&[0xff, 0xd8]) {
            return None;
        }
        let mut segments = Self {
            kept: Vec::with_capacity(jpeg.len()),
            frame: None,
            orientation: 1,
        };
        let (mut exif_seen, mut scanned) = (false, false);
        segments.kept.extend_from_slice(&jpeg[..2]);
        let mut at = 2;
        loop {
            // Fill bytes may pad any marker.
            while jpeg.get(at) == Some(&0xff) && jpeg.get(at + 1) == Some(&0xff) {
                at += 1;
            }
            if *jpeg.get(at)? != 0xff {
                return None;
            }
            let marker = *jpeg.get(at + 1)?;
            match marker {
                0xd9 => {
                    segments.kept.extend_from_slice(&[0xff, 0xd9]);
                    // A frame and at least one scan came first.
                    return (segments.frame.is_some() && scanned).then_some(segments);
                }
                // Restart markers belong inside a scan; a second start of
                // image doesn't belong anywhere.
                0xd0..=0xd8 | 0x01 => return None,
                _ => {}
            }
            let length = usize::from(u16::from_be_bytes([*jpeg.get(at + 2)?, *jpeg.get(at + 3)?]));
            if length < 2 {
                return None;
            }
            let end = at + 2 + length;
            let segment = jpeg.get(at..end)?;
            let body = &segment[4..];
            match marker {
                // Huffman-coded frames: baseline, extended, progressive.
                0xc0..=0xc2 => {
                    if segments.frame.is_some() || body.len() < 6 {
                        return None;
                    }
                    segments.frame = Some(Frame {
                        precision: body[0],
                        height: u16::from_be_bytes([body[1], body[2]]),
                        width: u16::from_be_bytes([body[3], body[4]]),
                        components: body[5],
                    });
                }
                // Lossless, hierarchical and arithmetic-coded frames are
                // left to the decoder.
                0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf => return None,
                0xe1 if !exif_seen && body.starts_with(b"Exif\0\0") => {
                    exif_seen = true;
                    if let Some(orientation) =
                        image::metadata::Orientation::from_exif_chunk(&body[6..])
                    {
                        segments.orientation = orientation.to_exif();
                    }
                }
                _ => {}
            }
            let metadata = matches!(marker, 0xe0..=0xed | 0xef | 0xfe);
            if !metadata {
                segments.kept.extend_from_slice(segment);
            }
            at = end;
            if marker == 0xda {
                // A scan before any frame has nothing to decode against.
                segments.frame.as_ref()?;
                scanned = true;
                // Entropy-coded data runs to the next marker that isn't a
                // stuffed byte or a restart.
                let data_end = scan_end(jpeg, at)?;
                segments.kept.extend_from_slice(&jpeg[at..data_end]);
                at = data_end;
            }
        }
    }
}

/// Where the entropy-coded data starting at `at` ends: at the first marker
/// other than a stuffed `FF 00`, a restart or fill.
fn scan_end(jpeg: &[u8], mut at: usize) -> Option<usize> {
    loop {
        at += next_marker_byte(jpeg.get(at..)?)?;
        match *jpeg.get(at + 1)? {
            0x00 | 0xd0..=0xd7 => at += 2,
            0xff => at += 1,
            _ => return Some(at),
        }
    }
}

/// Where the first `FF` byte in `bytes` is. A camera photo's scan data is
/// most of its file, so it is searched eight bytes at a time: the phones'
/// camera import must stay as fast as it was (#1024 decision 11).
fn next_marker_byte(bytes: &[u8]) -> Option<usize> {
    const ONES: u64 = 0x0101_0101_0101_0101;
    const HIGHS: u64 = 0x8080_8080_8080_8080;
    let mut skipped = 0;
    for chunk in bytes.as_chunks::<8>().0 {
        // A byte of `inverted` is zero where `chunk` holds FF.
        let inverted = !u64::from_ne_bytes(*chunk);
        if inverted.wrapping_sub(ONES) & !inverted & HIGHS != 0 {
            break;
        }
        skipped += 8;
    }
    bytes[skipped..]
        .iter()
        .position(|&byte| byte == 0xff)
        .map(|found| skipped + found)
}

#[cfg(test)]
mod tests {
    /// The word-at-a-time search finds what a byte-by-byte one does.
    #[test]
    fn the_next_ff_is_found_wherever_it_is() {
        for length in 0..40 {
            for at in 0..=length {
                let mut bytes = vec![0x7f; length];
                if at < length {
                    bytes[at] = 0xff;
                    // A byte just below FF is no match.
                    if at > 0 {
                        bytes[at - 1] = 0xfe;
                    }
                }
                let expected = bytes.iter().position(|&byte| byte == 0xff);
                assert_eq!(super::next_marker_byte(&bytes), expected, "{length} {at}");
            }
        }
    }
}
