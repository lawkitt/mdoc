//! What setup and Settings show for each downloadable model. Settings hold
//! plain model IDs; each engine knows its own manifest, languages and
//! downloads (ADR 0069), so these dispatch to the OCR module and `mdoc_detect`.
use mdoc_import::{model_download::Pending, ocr};
use mdoc_store::settings::Model;

/// Plain-language row title, e.g. "English & Russian" or "Standard".
pub fn title(model: Model) -> String {
    match model {
        Model::Ocr(m) => ocr::title(m),
        Model::Pii(m) => m.title().into(),
    }
}

/// How setup prompts name the bundle, e.g. "English OCR".
pub fn short_name(model: Model) -> String {
    match model {
        Model::Ocr(m) => format!("{} OCR", ocr::title(m)),
        Model::Pii(m) => format!("{} model", m.title()),
    }
}

pub fn pending(model: Model) -> Pending {
    match model {
        // Tests never read the real profile's installed models.
        Model::Ocr(m) => ocr::pending(ocr::root().ok().filter(|_| !cfg!(test)).as_deref(), m),
        Model::Pii(m) => mdoc_detect::pending(
            m,
            mdoc_detect::root_for(m)
                .ok()
                .filter(|_| !cfg!(test))
                .as_deref(),
            ocr::root().ok().filter(|_| !cfg!(test)).as_deref(),
        ),
    }
}

/// Attribution for exactly the files setup offers, including ONNX exports.
pub fn source_and_license(model: Model) -> (String, String) {
    match model {
        Model::Ocr(m) => ocr::source_and_license(m),
        Model::Pii(m) => mdoc_detect::source_and_license(m),
    }
}

pub fn attribution_label(model: Model) -> String {
    let (source, license) = source_and_license(model);
    format!("Source: {source}\nLicense: {license}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdoc_import::ocr::manifest;
    use mdoc_import::recognition::{ENGLISH, Language, RUSSIAN};
    use mdoc_store::settings::OcrModel;

    #[test]
    fn model_attribution_tracks_the_pinned_downloads() {
        for model in Model::ALL {
            let (source, license) = source_and_license(model);
            match model {
                Model::Pii(m) => {
                    let manifest = mdoc_detect::manifest_for(m);
                    assert_eq!(
                        source,
                        "Hugging Face · jugaadsrl/gliner2-privacy-filter-PII-multi-onnx"
                    );
                    assert_eq!(license, "Apache-2.0; encoder MIT");
                    for file in manifest.files {
                        assert!(file.url.starts_with(&format!(
                            "https://huggingface.co/{}/resolve/{}/",
                            manifest.repository, manifest.revision
                        )));
                    }
                }
                Model::Ocr(m) => {
                    let manifest = manifest::for_model(m);
                    assert_eq!(source, "GitHub · GreatV/oar-ocr");
                    assert_eq!(license, "Apache-2.0");
                    assert_eq!(
                        manifest.revision,
                        match m {
                            OcrModel::V6Small => "oar-ocr-v0.7.0",
                            OcrModel::Cyrillic => "oar-det-v0.7.0-rec-v0.3.0",
                        }
                    );
                    for file in manifest.artifacts {
                        assert!(file.url.starts_with(&format!(
                            "https://github.com/{}/releases/download/",
                            manifest.repository
                        )));
                    }
                }
            }
        }
    }

    #[test]
    fn pp_ocr_maps_recognition_languages_and_names_what_it_cannot_read() {
        // The card's offered lists come from the bundles, recommended first.
        assert_eq!(ocr::offered(), [&[ENGLISH][..], &[ENGLISH, RUSSIAN][..]]);
        assert_eq!(ocr::title(OcrModel::Cyrillic), "English & Russian");
        assert_eq!(ocr::plan(&[ENGLISH]), (OcrModel::V6Small, vec![]));
        assert_eq!(ocr::plan(&[RUSSIAN]), (OcrModel::Cyrillic, vec![]));
        assert_eq!(ocr::plan(&[ENGLISH, RUSSIAN]), (OcrModel::Cyrillic, vec![]));
        let german = Language("de");
        assert_eq!(
            ocr::plan(&[RUSSIAN, german]),
            (OcrModel::Cyrillic, vec![german])
        );
        assert_eq!(ocr::plan(&[german]), (OcrModel::V6Small, vec![german]));
    }
}
