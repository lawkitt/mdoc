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
/// comments go. Of APP14, only Adobe's colour-transform marker stays, and
/// rebuilt from its fixed fields: it holds no metadata and tells the
/// decoder how to read the colours. Anything after the end-of-image marker
/// (depth maps, appended previews) goes too.
///
/// A file whose scans use a quantization or Huffman table it never defines
/// can't be decoded, so it doesn't pass through: the decoder refuses it
/// instead of OCR reading a broken page.
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

struct Frame {
    precision: u8,
    height: u16,
    width: u16,
    components: u8,
    /// Each component's identifier and quantization table.
    tables: Vec<(u8, u8)>,
    progressive: bool,
}

/// The tables defined so far: quantization by identifier, Huffman by
/// class (0 DC, 1 AC) and identifier.
#[derive(Default)]
struct Tables {
    quantization: [bool; 4],
    huffman: [[bool; 4]; 2],
}

impl Tables {
    /// Record the tables a DQT segment's body defines.
    fn quantization(&mut self, mut body: &[u8]) -> Option<()> {
        while let Some((&head, rest)) = body.split_first() {
            let size = if head >> 4 == 0 { 64 } else { 128 };
            *self.quantization.get_mut(usize::from(head & 0x0f))? = true;
            body = rest.get(size..)?;
        }
        Some(())
    }

    /// Record the tables a DHT segment's body defines.
    fn huffman(&mut self, mut body: &[u8]) -> Option<()> {
        while let Some((&head, rest)) = body.split_first() {
            let counts = rest.get(..16)?;
            let values: usize = counts.iter().map(|&count| usize::from(count)).sum();
            let class = self.huffman.get_mut(usize::from(head >> 4))?;
            *class.get_mut(usize::from(head & 0x0f))? = true;
            body = rest.get(16 + values..)?;
        }
        Some(())
    }

    /// Whether every table the scan in an SOS segment's `body` uses is
    /// defined, for `frame`.
    fn cover(&self, frame: &Frame, body: &[u8]) -> Option<()> {
        let count = usize::from(*body.first()?);
        let selectors = body.get(1..1 + 2 * count)?;
        let spectral = body.get(1 + 2 * count..4 + 2 * count)?;
        let (start, approximation_high) = (spectral[0], spectral[2] >> 4);
        // Baseline scans and a progressive first DC scan decode DC values;
        // a progressive scan past DC decodes AC values; DC refinement needs
        // no table.
        let dc = !frame.progressive || (start == 0 && approximation_high == 0);
        let ac = !frame.progressive || start > 0;
        for selector in selectors.as_chunks::<2>().0 {
            let (_, quantization) = frame.tables.iter().find(|(id, _)| *id == selector[0])?;
            self.quantization
                .get(usize::from(*quantization))?
                .then_some(())?;
            if dc {
                self.huffman[0]
                    .get(usize::from(selector[1] >> 4))?
                    .then_some(())?;
            }
            if ac {
                self.huffman[1]
                    .get(usize::from(selector[1] & 0x0f))?
                    .then_some(())?;
            }
        }
        Some(())
    }
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
        let mut tables = Tables::default();
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
                    let components = body[5];
                    let tables = body
                        .get(6..6 + 3 * usize::from(components))?
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .map(|component| (component[0], component[2]))
                        .collect();
                    segments.frame = Some(Frame {
                        precision: body[0],
                        height: u16::from_be_bytes([body[1], body[2]]),
                        width: u16::from_be_bytes([body[3], body[4]]),
                        components,
                        tables,
                        progressive: marker == 0xc2,
                    });
                }
                0xdb => tables.quantization(body)?,
                0xc4 => tables.huffman(body)?,
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
            let metadata = matches!(marker, 0xe0..=0xef | 0xfe);
            if !metadata {
                segments.kept.extend_from_slice(segment);
            } else if marker == 0xee && body.len() >= 12 && body.starts_with(b"Adobe") {
                // Adobe's marker, rebuilt from its fixed fields (version,
                // two flag words, colour transform): nothing private after
                // them travels.
                segments.kept.extend_from_slice(&[0xff, 0xee, 0x00, 0x0e]);
                segments.kept.extend_from_slice(&body[..12]);
            }
            at = end;
            if marker == 0xda {
                // A scan before any frame has nothing to decode against,
                // nor one whose tables were never defined.
                tables.cover(segments.frame.as_ref()?, body)?;
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
        let found = jpeg.get(at..)?.iter().position(|&byte| byte == 0xff)?;
        at += found;
        match *jpeg.get(at + 1)? {
            0x00 | 0xd0..=0xd7 => at += 2,
            0xff => at += 1,
            _ => return Some(at),
        }
    }
}
