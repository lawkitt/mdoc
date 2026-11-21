//! A TIFF without its descriptive and private tags: every page (directory)
//! rewritten with only the tags that say how to draw it, and its strips or
//! tiles copied as they are. GPS, EXIF and XMP blocks, the camera, the
//! software, dates, names, descriptions, ICC profiles, sub-images and any
//! private tag go. The orientation stays, as for the other formats.
//!
//! Only classic TIFF is rewritten; BigTIFF, old-style JPEG and a file that
//! doesn't parse give `None`, which the phone then has its operating system
//! draw as an upright JPEG, or refuses.

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
/// Old-style JPEG, whose data hangs off tags this drops.
const OLD_JPEG: u32 = 6;
/// More pages than any real document has: a loop of directories stops here.
const MAX_PAGES: usize = 5_000;

/// The TIFF in `bytes`, its pages rewritten with only the [`KEPT`] tags.
pub(super) fn without_metadata(bytes: &[u8]) -> Option<Vec<u8>> {
    let order = match bytes.get(..4)? {
        [b'I', b'I', 42, 0] => Order::Little,
        [b'M', b'M', 0, 42] => Order::Big,
        _ => return None,
    };
    let mut out = bytes[..4].to_vec();
    // The first directory's offset, patched once it is written.
    let mut link = out.len();
    out.extend_from_slice(&[0; 4]);
    let mut next = order.u32(bytes, 4)? as usize;
    let mut pages = 0;
    while next != 0 {
        pages += 1;
        if pages > MAX_PAGES {
            return None;
        }
        let page = Directory::read(bytes, order, next)?;
        let (entries, following) = (page.entries.len(), page.next);
        let at = page.write(bytes, order, &mut out)?;
        order.put_u32(&mut out, link, u32::try_from(at).ok()?);
        link = at + 2 + 12 * entries;
        next = following as usize;
    }
    (pages > 0).then_some(out)
}

#[derive(Clone, Copy)]
enum Order {
    Little,
    Big,
}

impl Order {
    fn u16(self, bytes: &[u8], at: usize) -> Option<u16> {
        let raw: [u8; 2] = bytes.get(at..at + 2)?.try_into().ok()?;
        Some(match self {
            Self::Little => u16::from_le_bytes(raw),
            Self::Big => u16::from_be_bytes(raw),
        })
    }

    fn u32(self, bytes: &[u8], at: usize) -> Option<u32> {
        let raw: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
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

    /// Writes the page's image data, its values and then its directory at
    /// the end of `out`, the strip or tile offsets pointing at the copies;
    /// gives where the directory starts.
    fn write(mut self, bytes: &[u8], order: Order, out: &mut Vec<u8>) -> Option<usize> {
        if self
            .get(COMPRESSION)
            .and_then(|entry| entry.numbers(order))
            .is_some_and(|values| values.first() == Some(&OLD_JPEG))
        {
            return None;
        }
        for (offsets, counts) in [
            (STRIP_OFFSETS, STRIP_BYTE_COUNTS),
            (TILE_OFFSETS, TILE_BYTE_COUNTS),
        ] {
            let Some(old) = self.get(offsets).and_then(|entry| entry.numbers(order)) else {
                continue;
            };
            let sizes = self.get(counts)?.numbers(order)?;
            if sizes.len() != old.len() {
                return None;
            }
            let mut moved = Vec::with_capacity(old.len());
            for (offset, size) in old.into_iter().zip(sizes) {
                let (offset, size) = (offset as usize, size as usize);
                let data = bytes.get(offset..offset.checked_add(size)?)?;
                moved.push(u32::try_from(out.len()).ok()?);
                out.extend_from_slice(data);
                if out.len() % 2 == 1 {
                    out.push(0);
                }
            }
            let entry = self.entries.iter_mut().find(|entry| entry.tag == offsets)?;
            entry.kind = 4;
            entry.value = moved.iter().flat_map(|&offset| order.u32_bytes(offset)).collect();
        }
        // Values that don't fit in their entry go before the directory.
        let mut placed = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            if entry.value.len() > 4 {
                placed.push(Some(u32::try_from(out.len()).ok()?));
                out.extend_from_slice(&entry.value);
                if out.len() % 2 == 1 {
                    out.push(0);
                }
            } else {
                placed.push(None);
            }
        }
        let at = out.len();
        out.extend_from_slice(&order.u16_bytes(u16::try_from(self.entries.len()).ok()?));
        for (entry, place) in self.entries.iter().zip(placed) {
            out.extend_from_slice(&order.u16_bytes(entry.tag));
            out.extend_from_slice(&order.u16_bytes(entry.kind));
            out.extend_from_slice(&order.u32_bytes(entry.count));
            match place {
                Some(offset) => out.extend_from_slice(&order.u32_bytes(offset)),
                None => {
                    let mut inline = [0; 4];
                    inline[..entry.value.len()].copy_from_slice(&entry.value);
                    out.extend_from_slice(&inline);
                }
            }
        }
        // The next directory's offset, patched by the caller.
        out.extend_from_slice(&[0; 4]);
        Some(at)
    }
}
