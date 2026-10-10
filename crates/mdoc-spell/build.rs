//! Compress the bundled Hunspell dictionaries into OUT_DIR so the binary
//! carries them at a fraction of their text size; `lib.rs` inflates on load.
use std::{env, fs, io::Write, path::PathBuf};

const FILES: [&str; 6] = [
    "en_US.aff",
    "en_US.dic",
    "en_GB.aff",
    "en_GB.dic",
    "ru_RU.aff",
    "ru_RU.dic",
];

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    for name in FILES {
        let source = format!("dictionaries/{name}");
        println!("cargo:rerun-if-changed={source}");
        let bytes = fs::read(&source).unwrap_or_else(|e| panic!("{source}: {e}"));
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(&bytes).unwrap();
        fs::write(
            out.join(format!("{name}.deflate")),
            encoder.finish().unwrap(),
        )
        .unwrap();
    }
}
