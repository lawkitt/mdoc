//! What setup and Settings show for each downloadable model. Settings hold
//! plain model IDs; each engine knows its own manifest, languages and
//! downloads (ADR 0069), so these dispatch to the OCR module and `mdoc_detect`.
use mdoc_import::{model_download::Pending, ocr};
use mdoc_store::settings::Model;

pub fn check_status(result: Result<bool, String>) -> mdoc_store::model_state::Status {
    use mdoc_store::model_state::Status;
    if !ocr::SUPPORTED {
        return Status::Unavailable(
            "Automatic inference supports Apple Silicon macOS and Windows x64.".into(),
        );
    }
    match result {
        Ok(true) => Status::Ready,
        Ok(false) => Status::Missing,
        Err(error) => Status::Unavailable(error),
    }
}

pub fn setup_label(model: Model, remove: bool) -> &'static str {
    match (model, remove) {
        (Model::Ocr(_), false) => "installing text recognition",
        (Model::Ocr(_), true) => "removing text recognition",
        (Model::Pii(_), false) => "installing pseudonymization model",
        (Model::Pii(_), true) => "removing pseudonymization model",
    }
}

/// Metadata only; unit tests never inspect the real profile's installations.
pub fn files(model: Model) -> Vec<std::path::PathBuf> {
    if cfg!(test) {
        return Vec::new();
    }
    let Ok(root) = ocr::root() else {
        return Vec::new();
    };
    match model {
        Model::Ocr(m) => ocr::model_files(&root, m),
        Model::Pii(m) => mdoc_detect::root_for(m)
            .map(|path| mdoc_detect::model_files(&path, &root, m))
            .unwrap_or_default(),
    }
}

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
    fn setup_labels_name_the_model_job_in_other_windows() {
        for model in Model::ALL {
            match model {
                Model::Ocr(_) => {
                    assert_eq!(setup_label(model, false), "installing text recognition");
                    assert_eq!(setup_label(model, true), "removing text recognition");
                }
                Model::Pii(_) => {
                    assert_eq!(
                        setup_label(model, false),
                        "installing pseudonymization model"
                    );
                    assert_eq!(setup_label(model, true), "removing pseudonymization model");
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_runtime_open_failure_is_visible_beside_a_partial_download() {
        use mdoc_store::model_state::Status;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid-runtime.dll");
        std::fs::write(&path, b"not a Windows library").unwrap();
        let error = ocr::load_onnx_runtime(&path).unwrap_err();
        assert!(matches!(error, ocr::SetupError::RuntimeOpenWindows(_)));
        assert!(!error.is_missing());
        assert!(ocr::pending(Some(dir.path()), OcrModel::V6Small).total() > 0);
        assert_eq!(
            check_status(Err(error.to_string())),
            Status::Unavailable(error.to_string())
        );
    }

    #[test]
    fn removed_files_are_missing_through_real_reserved_checks() {
        let dir = tempfile::tempdir().unwrap();
        for model in OcrModel::ALL {
            let root = ocr::model_root(dir.path(), model);
            std::fs::create_dir_all(&root).unwrap();
            // The first runtime is checked before the model artifacts. A
            // corrupt file must report an error; removing that same file must
            // report Missing through that exact validation arm.
            let path = ocr::model_files(dir.path(), model)
                [ocr::manifest::for_model(model).artifacts.len()]
            .clone();
            std::fs::write(&path, b"removed fixture").unwrap();
            let config = mdoc_store::settings::OcrConfig {
                model,
                ..Default::default()
            };
            assert!(matches!(
                ocr::check_reserved_in(dir.path(), &config),
                Err(ocr::SetupError::Integrity(_))
            ));
            std::fs::remove_file(path).unwrap();
            let result = ocr::check_reserved_in(dir.path(), &config);
            assert!(result.as_ref().unwrap().is_none());
            if ocr::SUPPORTED {
                assert_eq!(
                    check_status(result.map(|i| i.is_some()).map_err(|e| e.to_string())),
                    mdoc_store::model_state::Status::Missing
                );
            }
        }
        for model in mdoc_store::settings::PiiModel::ALL {
            let path = dir
                .path()
                .join(&mdoc_detect::manifest_for(model).files[0].path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"removed fixture").unwrap();
            std::fs::remove_file(path).unwrap();
            let result = mdoc_detect::check_reserved_in(dir.path(), dir.path(), model);
            assert!(!result.as_ref().unwrap());
            if ocr::SUPPORTED {
                assert_eq!(
                    check_status(result.map_err(|e| e.to_string())),
                    mdoc_store::model_state::Status::Missing
                );
            }
        }
    }

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
