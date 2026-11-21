//! A TIFF without its descriptive and private tags: every page (directory)
//! rewritten with only the tags that say how to draw it, and its strips or
//! tiles copied once each. GPS, EXIF and XMP blocks, the camera, the
//! software, dates, names, descriptions, ICC profiles, sub-images and any
//! private tag go. The orientation stays, as for the other formats. A
//! JPEG-compressed page's strips or tiles and its `JPEGTables` lose their
//! APPn and comment segments (an EXIF block with GPS may hide there), their
//! coding tables kept.
//!
//! Only classic TIFF is rewritten; BigTIFF, old-style JPEG and a file that
//! doesn't parse are [`Rewrite::Unreadable`], which the phone then has its
//! operating system draw as an upright JPEG, or refuses. What the rewrite
//! writes is charged to a budget first: [`Rewrite::TooLarge`] past it.

use super::super::jpeg;

/// The tags kept: what a reader needs to draw each page, nothing more.
const KEPT: [u16; 33] = [
    254, // NewSubfileType
    255, // SubfileType
    256, // ImageWidth
    257, // ImageLength
    258, // BitsPerSample
    259, // Compression
    262, // PhotometricInterpretation
    266, // FillOrder
    273, // StripOffsets
    274, // Orientation
    277, // SamplesPerPixel
    278, // RowsPerStrip
    279, // StripByteCounts
    282, // XResolution
    283, // YResolution
    284, // PlanarConfiguration
    290, // GrayResponseUnit
    291, // GrayResponseCurve
    292, // T4Options
    293, // T6Options
    296, // ResolutionUnit
    317, // Predictor
    320, // ColorMap
    322, // TileWidth
    323, // TileLength
    324, // TileOffsets
    325, // TileByteCounts
    338, // ExtraSamples
    339, // SampleFormat
    347, // JPEGTables
    529, // YCbCrCoefficients
    530, // YCbCrSubSampling
    532, // ReferenceBlackWhite
];

const STRIP_OFFSETS: u16 = 273;
const STRIP_BYTE_COUNTS: u16 = 279;
const TILE_OFFSETS: u16 = 324;
const TILE_BYTE_COUNTS: u16 = 325;
const COMPRESSION: u16 = 259;
const JPEG_TABLES: u16 = 347;
/// Old-style JPEG, whose data hangs off tags this drops.
const OLD_JPEG: u32 = 6;
/// JPEG ("new style"): each strip or tile is a JPEG stream.
const JPEG: u32 = 7;
/// More pages than any real document has.
const MAX_PAGES: usize = 5_000;

/// Why a TIFF wasn't rewritten.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Rewrite {
    /// Not a classic TIFF this reads: BigTIFF, old-style JPEG, a loop of
    /// directories, or damage.
    Unreadable,
    /// The rewrite would outgrow its budget.
    TooLarge,
}

/// The TIFF in `bytes`, its pages rewritten with only the [`KEPT`] tags,
/// writing `budget` bytes at most.
pub(crate) fn without_metadata(bytes: &[u8], budget: usize) -> Result<Vec<u8>, Rewrite> {
    use Rewrite::Unreadable;
    let order = match bytes.get(..4).ok_or(Unreadable)? {
        [b'I', b'I', 42, 0] => Order::Little,
        [b'M', b'M', 0, 42] => Order::Big,
        _ => return Err(Unreadable),
    };
    let mut out = Output {
        bytes: bytes[..4].to_vec(),
        budget,
        copied: Vec::new(),
    };
    // The first directory's offset, patched once it is written.
    let mut link = out.bytes.len();
    out.push(&[0; 4])?;
    let mut next = order.u32(bytes, 4).ok_or(Unreadable)? as usize;
    let mut seen = Vec::new();
    while next != 0 {
        // A directory read twice is a loop.
        if seen.contains(&next) || seen.len() >= MAX_PAGES {
            return Err(Unreadable);
        }
        seen.push(next);
        let page = Directory::read(bytes, order, next).ok_or(Unreadable)?;
        let (entries, following) = (page.entries.len(), page.next);
        let at = page.write(bytes, order, &mut out)?;
        order.put_u32(
            &mut out.bytes,
            link,
            u32::try_from(at).map_err(|_| Rewrite::TooLarge)?,
        );
        link = at + 2 + 12 * entries;
        next = following as usize;
    }
    if seen.is_empty() {
        return Err(Unreadable);
    }
    Ok(out.bytes)
}

/// The rewritten file, charged before each write, and the image data
/// already copied (several directories may share a strip).
struct Output {
    bytes: Vec<u8>,
    budget: usize,
    /// (source offset, size, written at, written size) of copied data.
    copied: Vec<(usize, usize, u32, u32)>,
}

impl Output {
    fn push(&mut self, data: &[u8]) -> Result<(), Rewrite> {
        if self.bytes.len().saturating_add(data.len()) > self.budget {
            return Err(Rewrite::TooLarge);
        }
        self.bytes.extend_from_slice(data);
        Ok(())
    }

    /// Word alignment for what follows.
    fn align(&mut self) -> Result<(), Rewrite> {
        if self.bytes.len() % 2 == 1 {
            self.push(&[0])?;
        }
        Ok(())
    }

    fn at(&self) -> Result<u32, Rewrite> {
        u32::try_from(self.bytes.len()).map_err(|_| Rewrite::TooLarge)
    }
}

#[derive(Clone, Copy)]
enum Order {
    Little,
    Big,
}

impl Order {
    fn u16(self, bytes: &[u8], at: usize) -> Option<u16> {
        let raw: [u8; 2] = bytes.get(at..at.checked_add(2)?)?.try_into().ok()?;
        Some(match self {
            Self::Little => u16::from_le_bytes(raw),
            Self::Big => u16::from_be_bytes(raw),
        })
    }

    fn u32(self, bytes: &[u8], at: usize) -> Option<u32> {
        let raw: [u8; 4] = bytes.get(at..at.checked_add(4)?)?.try_into().ok()?;
        Some(match self {
            Self::Little => u32::from_le_bytes(raw),
            Self::Big => u32::from_be_bytes(raw),
        })
    }

    fn u16_bytes(self, value: u16) -> [u8; 2] {
        match self {
            Self::Little => value.to_le_bytes(),
            Self::Big => value.to_be_bytes(),
        }
    }

    fn u32_bytes(self, value: u32) -> [u8; 4] {
        match self {
            Self::Little => value.to_le_bytes(),
            Self::Big => value.to_be_bytes(),
        }
    }

    fn put_u32(self, out: &mut [u8], at: usize, value: u32) {
        out[at..at + 4].copy_from_slice(&self.u32_bytes(value));
    }
}

/// One kept tag: its type, count and value bytes, in the file's order.
struct Entry {
    tag: u16,
    kind: u16,
    count: u32,
    value: Vec<u8>,
}

impl Entry {
    /// The value's numbers, for SHORT and LONG.
    fn numbers(&self, order: Order) -> Option<Vec<u32>> {
        match self.kind {
            3 => (0..self.count as usize)
                .map(|index| order.u16(&self.value, 2 * index).map(u32::from))
                .collect(),
            4 => (0..self.count as usize)
                .map(|index| order.u32(&self.value, 4 * index))
                .collect(),
            _ => None,
        }
    }

    fn set_longs(&mut self, order: Order, values: &[u32]) {
        self.kind = 4;
        self.count = values.len() as u32;
        self.value = values
            .iter()
            .flat_map(|&value| order.u32_bytes(value))
            .collect();
    }
}

struct Directory {
    entries: Vec<Entry>,
    next: u32,
}

/// The size of one value of a TIFF field type.
fn type_size(kind: u16) -> Option<usize> {
    Some(match kind {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 => 4,
        5 | 10 | 12 => 8,
        _ => return None,
    })
}

impl Directory {
    fn read(bytes: &[u8], order: Order, at: usize) -> Option<Self> {
        let count = usize::from(order.u16(bytes, at)?);
        let mut entries = Vec::new();
        for index in 0..count {
            let entry = at + 2 + 12 * index;
            let tag = order.u16(bytes, entry)?;
            if !KEPT.contains(&tag) {
                continue;
            }
            let kind = order.u16(bytes, entry + 2)?;
            let count = order.u32(bytes, entry + 4)?;
            let size = type_size(kind)?.checked_mul(count as usize)?;
            let value = if size <= 4 {
                bytes.get(entry + 8..entry + 8 + size)?.to_vec()
            } else {
                let offset = order.u32(bytes, entry + 8)? as usize;
                bytes.get(offset..offset.checked_add(size)?)?.to_vec()
            };
            entries.push(Entry {
                tag,
                kind,
                count,
                value,
            });
        }
        let next = order.u32(bytes, at + 2 + 12 * count)?;
        Some(Self { entries, next })
    }

    fn get(&self, tag: u16) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.tag == tag)
    }

    /// Writes the page's image data (each strip or tile once, however many
    /// directories share it), its values and then its directory at the end
    /// of `out`, the offsets and counts pointing at the copies; gives where
    /// the directory starts.
    fn write(mut self, bytes: &[u8], order: Order, out: &mut Output) -> Result<usize, Rewrite> {
        use Rewrite::Unreadable;
        let compression = self
            .get(COMPRESSION)
            .and_then(|entry| entry.numbers(order))
            .and_then(|values| values.first().copied());
        if compression == Some(OLD_JPEG) {
            return Err(Unreadable);
        }
        let jpeg_coded = compression == Some(JPEG);
        if jpeg_coded
            && let Some(entry) = self
                .entries
                .iter_mut()
                .find(|entry| entry.tag == JPEG_TABLES)
        {
            entry.value = jpeg::without_markers(&entry.value).ok_or(Unreadable)?;
            entry.count = entry.value.len() as u32;
        }
        for (offsets, counts) in [
            (STRIP_OFFSETS, STRIP_BYTE_COUNTS),
            (TILE_OFFSETS, TILE_BYTE_COUNTS),
        ] {
            let Some(old) = self.get(offsets).and_then(|entry| entry.numbers(order)) else {
                continue;
            };
            let sizes = self
                .get(counts)
                .and_then(|entry| entry.numbers(order))
                .ok_or(Unreadable)?;
            if sizes.len() != old.len() {
                return Err(Unreadable);
            }
            let mut moved = Vec::with_capacity(old.len());
            let mut moved_sizes = Vec::with_capacity(old.len());
            for (offset, size) in old.into_iter().zip(sizes) {
                let (offset, size) = (offset as usize, size as usize);
                if let Some(&(_, _, at, written)) = out
                    .copied
                    .iter()
                    .find(|(from, length, _, _)| (*from, *length) == (offset, size))
                {
                    moved.push(at);
                    moved_sizes.push(written);
                    continue;
                }
                let data = bytes
                    .get(offset..offset.checked_add(size).ok_or(Unreadable)?)
                    .ok_or(Unreadable)?;
                let at = out.at()?;
                if jpeg_coded {
                    let stripped = jpeg::without_markers(data).ok_or(Unreadable)?;
                    out.push(&stripped)?;
                } else {
                    out.push(data)?;
                }
                let written = out.at()? - at;
                out.align()?;
                out.copied.push((offset, size, at, written));
                moved.push(at);
                moved_sizes.push(written);
            }
            for (tag, values) in [(offsets, &moved), (counts, &moved_sizes)] {
                self.entries
                    .iter_mut()
                    .find(|entry| entry.tag == tag)
                    .ok_or(Unreadable)?
                    .set_longs(order, values);
            }
        }
        // Values that don't fit in their entry go before the directory.
        let mut placed = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            if entry.value.len() > 4 {
                placed.push(Some(out.at()?));
                out.push(&entry.value)?;
                out.align()?;
            } else {
                placed.push(None);
            }
        }
        let at = out.bytes.len();
        let count = u16::try_from(self.entries.len()).map_err(|_| Unreadable)?;
        out.push(&order.u16_bytes(count))?;
        for (entry, place) in self.entries.iter().zip(placed) {
            out.push(&order.u16_bytes(entry.tag))?;
            out.push(&order.u16_bytes(entry.kind))?;
            out.push(&order.u32_bytes(entry.count))?;
            match place {
                Some(offset) => out.push(&order.u32_bytes(offset))?,
                None => {
                    let mut inline = [0; 4];
                    inline[..entry.value.len()].copy_from_slice(&entry.value);
                    out.push(&inline)?;
                }
            }
        }
        // The next directory's offset, patched by the caller.
        out.push(&[0; 4])?;
        Ok(at)
    }
}
