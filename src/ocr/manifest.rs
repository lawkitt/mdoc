//! App attribution around the converter's pinned artifact manifest. URLs,
//! sizes and hashes stay owned by pdf-inspector, not copied into this layer.
use pdf_inspector::vision::{ModelManifest, PP_OCR_CYRILLIC, PP_OCR_V6_SMALL};

pub struct Manifest {
    pub model: ModelManifest,
    pub repository: &'static str,
    pub license: &'static str,
}

impl std::ops::Deref for Manifest {
    type Target = ModelManifest;

    fn deref(&self) -> &Self::Target {
        &self.model
    }
}

// LICENSE checked at the artifact releases' commits:
// v0.7.0: https://github.com/GreatV/oar-ocr/blob/4e350472b058ef78a640634bc9095fc3836c3e34/LICENSE
// v0.3.0: https://github.com/GreatV/oar-ocr/blob/7a0ea9cbe5de7d9e4c8415b032f4854fb688245e/LICENSE
const SMALL: Manifest = Manifest {
    model: PP_OCR_V6_SMALL,
    repository: "GreatV/oar-ocr",
    license: "Apache-2.0",
};
const CYRILLIC: Manifest = Manifest {
    model: PP_OCR_CYRILLIC,
    repository: "GreatV/oar-ocr",
    license: "Apache-2.0",
};

pub fn for_model(model: mdoc_store::settings::OcrModel) -> &'static Manifest {
    match model {
        mdoc_store::settings::OcrModel::V6Small => &SMALL,
        mdoc_store::settings::OcrModel::Cyrillic => &CYRILLIC,
    }
}
