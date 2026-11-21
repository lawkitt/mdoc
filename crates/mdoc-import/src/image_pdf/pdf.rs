//! The PDF around the pages: one image XObject per page, or one per band
//! of rows when a page comes in bands (a fax's CCITT strips), drawn upright
//! by the page's content matrix. Pages are written as they come, so only
//! the PDF and the page at hand are held.

use super::{ImageError, MAX_LONG_SIDE, Page, Stream};

/// Appends each page directly to the PDF and keeps only its object offsets.
pub(super) struct Writer {
    pdf: Pdf,
    dpi: u16,
    /// Each page's object number, in order.
    kids: Vec<usize>,
}

impl Writer {
    /// A PDF whose pages are each sized by their own resolution when they
    /// state one, else so that an image pixel is a page pixel at `dpi`;
    /// none longer than [`MAX_LONG_SIDE`] pixels at `dpi`. It grows to
    /// `limit` bytes at most.
    pub(super) fn new(dpi: u16, limit: usize) -> Result<Self, ImageError> {
        let mut pdf = Pdf {
            bytes: Vec::new(),
            offsets: vec![0; 2],
            limit,
        };
        pdf.append(&[b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n"])?;
        pdf.object(1, "<< /Type /Catalog /Pages 2 0 R >>", None)?;
        Ok(Self {
            pdf,
            dpi,
            kids: Vec::new(),
        })
    }

    pub(super) fn push(&mut self, page: Page) -> Result<(), ImageError> {
        // Objects 1 and 2 are the catalog and page tree. Each page takes the
        // page, its content stream and its images (one per band).
        let bands = page.bands();
        let first = self.pdf.offsets.len() + 1;
        self.pdf.offsets.resize(first + 1 + bands.len(), 0);
        self.kids.push(first);
        let pdf = &mut self.pdf;
        // A page that states its resolution (a TIFF) has its own physical
        // size; any other is its pixels at the PDF's resolution. Either is
        // at most MAX_LONG_SIDE pixels long when rendered at `dpi`: a large
        // sheet is made smaller rather than have OCR render more pixels
        // than the page holds (decision 10's bound).
        let fallback = f64::from(self.dpi.max(1));
        let (x_dpi, y_dpi) = page.resolution.unwrap_or((fallback, fallback));
        let (width, height) = (
            f64::from(page.width) * 72. / x_dpi,
            f64::from(page.height) * 72. / y_dpi,
        );
        let rendered = width.max(height) / 72. * fallback;
        let fit = (f64::from(MAX_LONG_SIDE) / rendered).min(1.);
        let (width, height) = (width * fit, height * fit);
        let [a, b, c, d, e, f] = upright(page.orientation, width, height);
        // Orientations 5 to 8 turn the image a quarter, swapping its sides.
        let (page_width, page_height) = if page.orientation >= 5 {
            (height, width)
        } else {
            (width, height)
        };
        let images = (0..bands.len())
            .map(|index| format!("/Im{index} {} 0 R", first + 2 + index))
            .collect::<Vec<_>>()
            .join(" ");
        pdf.object(
            first,
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] \
                 /Resources << /XObject << {images} >> >> /Contents {} 0 R >>",
                number(page_width),
                number(page_height),
                first + 1
            ),
            None,
        )?;
        // Each band fills its rows of the image's unit square, the first
        // band at the top. Its matrix is that place composed with the
        // upright matrix here, so that its edges are plain numbers of
        // points: PDFium draws an image over the outer pixels of its edges,
        // and a band placed by a rounded share stretched by a pixel, its
        // rows moved.
        let rows = f64::from(page.height);
        let mut below = page.height;
        let content = bands
            .iter()
            .enumerate()
            .map(|(index, band)| {
                below -= band.rows;
                let (share, bottom) = (f64::from(band.rows), f64::from(below));
                let matrix = [
                    a,
                    b,
                    c * share / rows,
                    d * share / rows,
                    c * bottom / rows + e,
                    d * bottom / rows + f,
                ]
                .map(number)
                .join(" ");
                format!("q {matrix} cm /Im{index} Do Q")
            })
            .collect::<Vec<_>>()
            .join(" ");
        pdf.object(
            first + 1,
            &format!("<< /Length {} >>", content.len()),
            Some(content.as_bytes()),
        )?;
        let color = if page.gray {
            "/DeviceGray"
        } else {
            "/DeviceRGB"
        };
        let mut at = 0;
        for (index, band) in bands.iter().enumerate() {
            let data = &page.data[at..at + band.bytes];
            at += band.bytes;
            let filter = match &page.stream {
                Stream::Jpeg => "/DCTDecode".to_owned(),
                Stream::Flate => "/FlateDecode".to_owned(),
                Stream::Ccitt(ccitt) => format!(
                    "/CCITTFaxDecode /DecodeParms << /K {} /Columns {} /Rows {} /BlackIs1 {} \
                     /EncodedByteAlign {} /EndOfBlock false >>",
                    ccitt.k, page.width, band.rows, ccitt.black_is_1, ccitt.byte_align
                ),
            };
            pdf.object(
                first + 2 + index,
                &format!(
                    "<< /Type /XObject /Subtype /Image /Width {} /Height {} \
                     /ColorSpace {color} /BitsPerComponent {} /Filter {filter} /Length {} >>",
                    page.width,
                    band.rows,
                    page.bits,
                    data.len()
                ),
                Some(data),
            )?;
        }
        Ok(())
    }

    /// The PDF and its page count.
    pub(super) fn finish(mut self) -> Result<(Vec<u8>, u32), ImageError> {
        let count = self.kids.len();
        let kids = self
            .kids
            .iter()
            .map(|id| format!("{id} 0 R"))
            .collect::<Vec<_>>()
            .join(" ");
        self.pdf.object(
            2,
            &format!("<< /Type /Pages /Kids [{kids}] /Count {count} >>"),
            None,
        )?;
        Ok((self.pdf.finish()?, u32::try_from(count).unwrap_or(u32::MAX)))
    }
}

/// A PDF of `pages` at once, as [`Writer`] makes it: for tests.
#[cfg(test)]
pub(super) fn write(pages: &[Page], dpi: u16) -> Vec<u8> {
    let mut writer = Writer::new(dpi, usize::MAX).expect("no limit");
    for page in pages {
        writer.push(page.clone()).expect("no limit");
    }
    writer.finish().expect("no limit").0
}

/// The matrix that draws an image's unit square upright for EXIF
/// `orientation`, the image stored `width` by `height` points. A PDF image's
/// first row is at the top of its unit square (v = 1).
pub(super) fn upright(orientation: u8, width: f64, height: f64) -> [f64; 6] {
    let (w, h) = (width, height);
    match orientation {
        // Mirrored left to right.
        2 => [-w, 0., 0., h, w, 0.],
        // Turned half round.
        3 => [-w, 0., 0., -h, w, h],
        // Mirrored top to bottom.
        4 => [w, 0., 0., -h, 0., h],
        // Mirrored along the top-left to bottom-right diagonal.
        5 => [0., -w, -h, 0., h, w],
        // Turned a quarter clockwise.
        6 => [0., -w, h, 0., 0., w],
        // Mirrored along the other diagonal.
        7 => [0., w, h, 0., 0., 0.],
        // Turned a quarter anticlockwise.
        8 => [0., w, -h, 0., h, 0.],
        _ => [w, 0., 0., h, 0., 0.],
    }
}

/// A PDF number: up to four decimals, without trailing zeros.
fn number(value: f64) -> String {
    let text = format!("{value:.4}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    match text {
        "-0" | "" => "0".into(),
        text => text.into(),
    }
}

struct Pdf {
    bytes: Vec<u8>,
    offsets: Vec<usize>,
    limit: usize,
}

impl Pdf {
    fn object(&mut self, id: usize, head: &str, stream: Option<&[u8]>) -> Result<(), ImageError> {
        self.offsets[id - 1] = self.bytes.len();
        let start = format!("{id} 0 obj\n{head}");
        let end = b"\nendobj\n";
        if let Some(stream) = stream {
            self.append(&[start.as_bytes(), b"\nstream\n", stream, b"\nendstream", end])
        } else {
            self.append(&[start.as_bytes(), end])
        }
    }

    fn append(&mut self, parts: &[&[u8]]) -> Result<(), ImageError> {
        let end = self.bytes.len() + parts.iter().map(|bytes| bytes.len()).sum::<usize>();
        if end > self.limit {
            return Err(ImageError::PdfTooLarge);
        }
        let capacity = self.bytes.capacity();
        if end > capacity {
            let next = end
                .saturating_add(4096)
                .max(capacity.saturating_mul(2))
                .min(self.limit);
            self.bytes.reserve_exact(next - self.bytes.len());
        }
        for part in parts {
            self.bytes.extend_from_slice(part);
        }
        Ok(())
    }

    fn finish(mut self) -> Result<Vec<u8>, ImageError> {
        let xref = self.bytes.len();
        let size = self.offsets.len() + 1;
        self.append(&[format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes()])?;
        for offset in std::mem::take(&mut self.offsets) {
            self.append(&[format!("{offset:010} 00000 n \n").as_bytes()])?;
        }
        self.append(&[format!(
            "trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
        )
        .as_bytes()])?;
        Ok(self.bytes)
    }
}
