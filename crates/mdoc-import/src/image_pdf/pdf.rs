//! The PDF around the pages: one image XObject per page, drawn upright by
//! the page's content matrix.

use super::{Page, Stream};

/// A PDF with `pages` in order, each sized so that an image pixel is a page
/// pixel at `dpi`.
pub(super) fn write(pages: &[Page], dpi: u16) -> Vec<u8> {
    let mut pdf = Pdf::default();
    let count = pages.len();
    // 1: catalog, 2: page tree, then three objects per page: the page, its
    // image and its content stream.
    pdf.object("<< /Type /Catalog /Pages 2 0 R >>", None);
    let kids = (0..count)
        .map(|index| format!("{} 0 R", 3 + 3 * index))
        .collect::<Vec<_>>()
        .join(" ");
    pdf.object(
        &format!("<< /Type /Pages /Kids [{kids}] /Count {count} >>"),
        None,
    );
    for (index, page) in pages.iter().enumerate() {
        let first = 3 + 3 * index;
        let scale = 72. / f64::from(dpi.max(1));
        let (width, height) = (
            f64::from(page.width) * scale,
            f64::from(page.height) * scale,
        );
        let [a, b, c, d, e, f] = upright(page.orientation, width, height);
        // Orientations 5 to 8 turn the image a quarter, swapping its sides.
        let (page_width, page_height) = if page.orientation >= 5 {
            (height, width)
        } else {
            (width, height)
        };
        pdf.object(
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] \
                 /Resources << /XObject << /Im0 {} 0 R >> >> /Contents {} 0 R >>",
                number(page_width),
                number(page_height),
                first + 1,
                first + 2
            ),
            None,
        );
        let filter = match page.stream {
            Stream::Jpeg => "/DCTDecode",
            Stream::Flate => "/FlateDecode",
        };
        let color = if page.gray {
            "/DeviceGray"
        } else {
            "/DeviceRGB"
        };
        pdf.object(
            &format!(
                "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {color} \
                 /BitsPerComponent 8 /Filter {filter} /Length {} >>",
                page.width,
                page.height,
                page.data.len()
            ),
            Some(&page.data),
        );
        let content = format!(
            "q {} {} {} {} {} {} cm /Im0 Do Q",
            number(a),
            number(b),
            number(c),
            number(d),
            number(e),
            number(f)
        );
        pdf.object(
            &format!("<< /Length {} >>", content.len()),
            Some(content.as_bytes()),
        );
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
