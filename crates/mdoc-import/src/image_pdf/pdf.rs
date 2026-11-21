//! The PDF around the pages: one image XObject per page, or one per band
//! of rows when a page comes in bands (a fax's CCITT strips), drawn upright
//! by the page's content matrix.

use super::{MAX_LONG_SIDE, Page, Stream};

/// A PDF with `pages` in order, each sized by its own resolution when it
/// states one, else so that an image pixel is a page pixel at `dpi`; none
/// longer than [`MAX_LONG_SIDE`] pixels at `dpi`.
pub(super) fn write(pages: &[Page], dpi: u16) -> Vec<u8> {
    let mut pdf = Pdf::default();
    let count = pages.len();
    // 1: catalog, 2: page tree, then for each page: the page, its content
    // stream, and its images.
    let mut first_ids = Vec::with_capacity(count);
    let mut next = 3;
    for page in pages {
        first_ids.push(next);
        next += 2 + page.bands().len();
    }
    pdf.object("<< /Type /Catalog /Pages 2 0 R >>", None);
    let kids = first_ids
        .iter()
        .map(|id| format!("{id} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    pdf.object(
        &format!("<< /Type /Pages /Kids [{kids}] /Count {count} >>"),
        None,
    );
    for (page, &first) in pages.iter().zip(&first_ids) {
        // A page that states its resolution (a TIFF) has its own physical
        // size; any other is its pixels at the PDF's resolution. Either is
        // at most MAX_LONG_SIDE pixels long when rendered at `dpi`: a large
        // sheet is made smaller rather than have OCR render more pixels
        // than the page holds (decision 10's bound).
        let fallback = f64::from(dpi.max(1));
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
        let bands = page.bands();
        let images = (0..bands.len())
            .map(|index| format!("/Im{index} {} 0 R", first + 2 + index))
            .collect::<Vec<_>>()
            .join(" ");
        pdf.object(
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] \
                 /Resources << /XObject << {images} >> >> /Contents {} 0 R >>",
                number(page_width),
                number(page_height),
                first + 1
            ),
            None,
        );
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
            &format!("<< /Length {} >>", content.len()),
            Some(content.as_bytes()),
        );
        let color = if page.gray {
            "/DeviceGray"
        } else {
            "/DeviceRGB"
        };
        let mut at = 0;
        for band in &bands {
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
                &format!(
                    "<< /Type /XObject /Subtype /Image /Width {} /Height {} \
                     /ColorSpace {color} /BitsPerComponent {} /Filter {filter} /Length {} >>",
                    page.width,
                    band.rows,
                    page.bits,
                    data.len()
                ),
                Some(data),
            );
        }
    }
    pdf.finish()
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

#[derive(Default)]
struct Pdf {
    bytes: Vec<u8>,
    offsets: Vec<usize>,
}

impl Pdf {
    fn object(&mut self, head: &str, stream: Option<&[u8]>) {
        if self.bytes.is_empty() {
            self.bytes
                .extend_from_slice(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n");
        }
        self.offsets.push(self.bytes.len());
        self.bytes
            .extend_from_slice(format!("{} 0 obj\n{head}", self.offsets.len()).as_bytes());
        if let Some(stream) = stream {
            self.bytes.extend_from_slice(b"\nstream\n");
            self.bytes.extend_from_slice(stream);
            self.bytes.extend_from_slice(b"\nendstream");
        }
        self.bytes.extend_from_slice(b"\nendobj\n");
    }

    fn finish(mut self) -> Vec<u8> {
        let xref = self.bytes.len();
        let size = self.offsets.len() + 1;
        self.bytes
            .extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
        for offset in &self.offsets {
            self.bytes
                .extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        self.bytes.extend_from_slice(
            format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n")
                .as_bytes(),
        );
        self.bytes
    }
}
