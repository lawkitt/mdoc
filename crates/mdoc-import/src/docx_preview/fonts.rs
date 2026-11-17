//! The fallback fonts the app ships for documents whose fonts are missing:
//! Carlito for Calibri, Caladea for Cambria, and Liberation Sans, Serif and
//! Mono, metric-compatible with Arial, Times New Roman and Courier New, all
//! under the SIL Open Font License, which sits beside them in `resources/fonts`.
//!
//! docxide looks a run's own font up first and reaches Carlito and Caladea
//! only when Calibri or Cambria is not installed, so installed fonts always
//! win. The directory is a conversion option (`DOCXIDE_FONTS` is the same
//! setting read from the environment, which the worker leaves alone): docxide
//! searches it before the system directories, which matters only for a family
//! of the same name, such as a Carlito the user installed.
use std::path::{Path, PathBuf};

/// A file every copy of the directory has.
const SENTINEL: &str = "Carlito-Regular.ttf";

pub(super) fn options() -> docxide_pdf::ConvertOptions {
    let options = docxide_pdf::ConvertOptions::new();
    match bundled() {
        Some(dir) => options.font_dirs([dir]),
        None => options,
    }
}

/// The bundled fonts: beside the executable on Windows, in the bundle's
/// Resources on macOS, and in the source tree for a development build.
pub(super) fn bundled() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let source = cfg!(debug_assertions)
        .then(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources/fonts"));
    [exe_dir.join("fonts"), exe_dir.join("../Resources/fonts")]
        .into_iter()
        .chain(source)
        .find(|dir| dir.join(SENTINEL).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every family the decision names, in its four styles, with a licence.
    #[test]
    fn the_bundle_has_every_face_and_its_licences() {
        let dir = bundled().expect("a development build finds resources/fonts");
        for family in [
            "Carlito",
            "Caladea",
            "LiberationSans",
            "LiberationSerif",
            "LiberationMono",
        ] {
            for style in ["Regular", "Bold", "Italic", "BoldItalic"] {
                let face = dir.join(format!("{family}-{style}.ttf"));
                assert!(face.is_file(), "{} is missing", face.display());
            }
        }
        for licence in [
            "Carlito-OFL.txt",
            "Caladea-OFL.txt",
            "Liberation-LICENSE.txt",
        ] {
            assert!(dir.join(licence).is_file(), "{licence} is missing");
        }
    }
}
