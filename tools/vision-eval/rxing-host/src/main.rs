//! rxing-host: decode every QR code and barcode rxing finds on page images.
//!
//! Usage: rxing-host <image.png>...
//! Prints one JSON line per image: {"image", "width", "height", "ms",
//! "codes": [{"format", "text", "points": [[x, y], ...]}]} or {"image", "error"}.
//! Timing covers detection on the decoded luma image only.

use std::time::Instant;

use rxing::{BarcodeFormat, DecodeHints, DecodeHintValue};
use serde_json::{json, Value};

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: rxing-host <image.png>...");
        std::process::exit(2);
    }
    for path in paths {
        println!("{}", run(&path));
    }
}

fn run(path: &str) -> Value {
    let img = match image::open(path) {
        Ok(img) => img.into_luma8(),
        Err(e) => return json!({"image": path, "error": e.to_string()}),
    };
    let (w, h) = img.dimensions();
    let mut hints = DecodeHints::default().with(DecodeHintValue::TryHarder(true));
    hints = hints.with(DecodeHintValue::AlsoInverted(true));
    let started = Instant::now();
    let found = rxing::helpers::detect_multiple_in_luma_with_hints(img.into_raw(), w, h, &mut hints);
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let codes: Vec<Value> = match found {
        Ok(results) => results
            .iter()
            .map(|r| {
                let points: Vec<Value> = r.getPoints().iter().map(|p| json!([p.x, p.y])).collect();
                json!({
                    "format": format_name(r.getBarcodeFormat()),
                    "text": r.getText(),
                    "points": points,
                })
            })
            .collect(),
        Err(_) => Vec::new(), // NotFound and friends: no code on the page
    };
    json!({"image": path, "width": w, "height": h, "ms": ms, "codes": codes})
}

fn format_name(f: &BarcodeFormat) -> String {
    format!("{f:?}")
}
