//! Immutable local QA input. These results never enter the document/review owners.
use crate::{
    ocr, pii,
    settings::{OcrConfig, PiiConfig},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

#[derive(Clone)]
pub enum Input {
    Pdf(Arc<Vec<u8>>),
    Markdown(Arc<String>),
}
impl Input {
    pub fn hash(&self) -> String {
        let bytes: &[u8] = match self {
            Self::Pdf(b) => b,
            Self::Markdown(s) => s.as_bytes(),
        };
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    }
    pub fn pdf(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take(256 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 256 * 1024 * 1024 {
            return Err("PDF comparison supports up to 256 MiB.".into());
        }
        if !bytes[..bytes.len().min(1024)]
            .windows(5)
            .any(|b| b == b"%PDF-")
        {
            return Err("OCR comparison requires an original PDF.".into());
        }
        Ok(Self::Pdf(Arc::new(bytes)))
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Config {
    Ocr(OcrConfig),
    Pii(PiiConfig),
}
impl Config {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Ocr(c) => c.model.name(),
            Self::Pii(c) => c.model.name(),
        }
    }
    pub fn summary(&self) -> String {
        match self {
            Self::Ocr(c) => format!(
                "{} · {} DPI · minimum confidence {} · Force · CPU",
                c.model.name(),
                c.dpi,
                c.minimum_confidence
            ),
            Self::Pii(c) => format!("{} · threshold {} · CPU", c.model.name(), c.threshold),
        }
    }
    pub fn identity(&self) -> serde_json::Value {
        match self {
            Self::Ocr(c) => {
                let m = c.model.manifest();
                serde_json::json!({"model":m.id,"revision":m.revision,"mode":"Force","runtime":"ONNX Runtime 1.27.0","renderer":"PDFium native-v7988","artifacts":m.artifacts.iter().map(|a|serde_json::json!({"file":a.filename,"sha256":a.sha256})).collect::<Vec<_>>()})
            }
            Self::Pii(c) => {
                let m = pii::detector::manifest_for(c.model);
                serde_json::json!({"model":m.id,"revision":m.revision,"repository":m.repository,"runtime":"ONNX Runtime 1.27.0","engine":c.model.engine(),"model_card":c.model.hugging_face_url(),"languages":c.model.languages(),"description":c.model.description(),"confidence":"GLiNER2 span confidence","execution":"CPU","threads":4,"artifacts":m.files.iter().map(|a|serde_json::json!({"file":a.path,"sha256":a.sha256})).collect::<Vec<_>>()})
            }
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct ResultRecord {
    pub configuration: Config,
    pub identity: serde_json::Value,
    pub input_sha256: String,
    pub pages: Option<BTreeSet<u32>>,
    pub elapsed_ms: u128,
    pub output: serde_json::Value,
    pub error: Option<String>,
}
impl ResultRecord {
    pub fn measurements(&self) -> String {
        if let Some(count) = self.output.get("count") {
            return format!("{count} candidates · confidence and count are not accuracy scores");
        }
        self.output.get("pages").and_then(|v|v.as_array()).map(|pages|pages.iter().map(|p|format!("Page {} · render {} ms · recognition {} ms · preparation {} ms · warnings {}",p["page"],p["render_ms"],p["ocr_ms"],p["assembly_ms"],p["warnings"])).collect::<Vec<_>>().join("\n")).unwrap_or_default()
    }
    pub fn text(&self, raw: bool) -> String {
        if let Some(error) = &self.error {
            return error.clone();
        }
        let key = if raw {
            "recognition_text"
        } else {
            "prepared_markdown"
        };
        if let Some(text) = self.output.get(key).and_then(|v| v.as_str()) {
            return text.to_owned();
        }
        self.output
            .get("predictions")
            .and_then(|v| v.as_array())
            .map(|predictions| {
                predictions
                    .iter()
                    .map(|p| {
                        format!(
                            "{}  [{}]  {:.3}  bytes {}–{}",
                            p["text"].as_str().unwrap_or(""),
                            p["category"].as_str().unwrap_or(""),
                            p["confidence"].as_f64().unwrap_or_default(),
                            p["start"],
                            p["end"]
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }
}
pub fn parse_pages(text: &str) -> Result<Option<BTreeSet<u32>>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let mut pages = BTreeSet::new();
    for part in text.split(',') {
        let part = part.trim();
        let (from, to) = if let Some((a, b)) = part.split_once('-') {
            (a.trim().parse::<u32>(), b.trim().parse::<u32>())
        } else {
            let n = part.parse::<u32>();
            (n.clone(), n)
        };
        let from = from.map_err(|_| "Use page numbers such as 1, 3-5.".to_string())?;
        let to = to.map_err(|_| "Use page numbers such as 1, 3-5.".to_string())?;
        if from == 0 || from > to || to - from > 10000 {
            return Err("Invalid page range; use positive, ascending page numbers.".into());
        }
        pages.extend(from..=to);
        if pages.len() > 10000 {
            return Err("Page selection is too large.".into());
        }
    }
    Ok(Some(pages))
}
pub fn run(
    input: &Input,
    config: Config,
    pages: Option<BTreeSet<u32>>,
    cancel: &AtomicBool,
) -> ResultRecord {
    let started = Instant::now();
    let output = (|| {
        if cancel.load(Ordering::Relaxed) {
            return Err("Comparison cancelled.".into());
        }
        match (input, &config) {
            (Input::Markdown(source), Config::Pii(c)) => {
                let detections = pii::detector::scan_reserved(source, cancel, c)?;
                let predictions=detections.iter().map(|d|serde_json::json!({"text":&source[d.range.clone()],"category":format!("{:?}",d.category),"start":d.range.start,"end":d.range.end,"confidence":d.score})).collect::<Vec<_>>();
                Ok(
                    serde_json::json!({"predictions":predictions,"count":predictions.len(),"warnings":[c.model.evidence(),c.model.languages(),c.model.description(),"Confidence and candidate count are not accuracy scores."]}),
                )
            }
            (Input::Pdf(bytes), Config::Ocr(c)) => {
                let installed = ocr::installed_config(c)?;
                let mut options = installed.options();
                options.page_numbers = pages.clone();
                options.retain_recognition = true;
                let result = pdf_inspector::vision::process_pdf_with_ocr_mem(bytes, options)
                    .map_err(|e| e.to_string())?;
                let recognition_text = result
                    .recognition
                    .iter()
                    .map(|p| {
                        format!(
                            "Page {}\n{}",
                            p.page_number,
                            p.spans
                                .iter()
                                .map(|s| s.text.as_str())
                                .collect::<Vec<_>>()
                                .join("\n")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n");
                let raw=result.recognition.iter().map(|p|serde_json::json!({"page":p.page_number,"confidence":p.mean_confidence,"elapsed_ms":p.processing_time_ms,"warnings":p.warnings,"spans":p.spans.iter().map(|s|serde_json::json!({"text":s.text,"confidence":s.confidence,"polygon":s.polygon.points.iter().map(|v|[v.x,v.y]).collect::<Vec<_>>()})).collect::<Vec<_>>() })).collect::<Vec<_>>();
                let prepared=result.pages.iter().map(|p|serde_json::json!({"page":p.page_number,"source":format!("{:?}",p.provenance.source),"warnings":p.provenance.warnings,"render_ms":p.provenance.timings.render_ms,"ocr_ms":p.provenance.timings.ocr_ms,"assembly_ms":p.provenance.timings.assembly_ms})).collect::<Vec<_>>();
                Ok(
                    serde_json::json!({"recognition_text":recognition_text,"recognition":raw,"prepared_markdown":result.markdown,"pages":prepared,"warnings":c.model.evidence(),"render_ms":result.render_time_ms,"ocr_ms":result.ocr_time_ms}),
                )
            }
            _ => Err("Configuration does not match the captured input.".into()),
        }
    })();
    let (output, error) = match output {
        Ok(output) if !cancel.load(Ordering::Relaxed) => (output, None),
        Ok(_) => (
            serde_json::Value::Null,
            Some("Comparison cancelled.".into()),
        ),
        Err(e) => (serde_json::Value::Null, Some(e)),
    };
    ResultRecord {
        identity: config.identity(),
        configuration: config,
        input_sha256: input.hash(),
        pages,
        elapsed_ms: started.elapsed().as_millis(),
        output,
        error,
    }
}
pub fn export_path_is_source(path: &Path, sources: &[PathBuf]) -> bool {
    sources.iter().any(|source| {
        path == source || (path.exists() && path.canonicalize().ok() == source.canonicalize().ok())
    })
}
pub fn save_report(path: &Path, records: &[ResultRecord]) -> Result<(), String> {
    let data = serde_json::json!({"version":1,"application":"mdoc","build":env!("CARGO_PKG_VERSION"),"profile":if cfg!(debug_assertions){"debug"}else{"release"},"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"runs":records});
    let parent = path.parent().ok_or("Invalid report path")?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut file, &data).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn probe_model() -> crate::settings::Model {
        match std::env::var("MDOC_MODEL")
            .expect("set MDOC_MODEL explicitly")
            .as_str()
        {
            "ocr-cyrillic" => crate::settings::Model::Ocr(crate::settings::OcrModel::Cyrillic),
            "ocr-v6" => crate::settings::Model::Ocr(crate::settings::OcrModel::V6Small),
            "pii-fp16" => crate::settings::Model::Pii(crate::settings::PiiModel::Fp16),
            "pii-fp32" => crate::settings::Model::Pii(crate::settings::PiiModel::Fp32),
            _ => panic!("unknown MDOC_MODEL"),
        }
    }
    #[test]
    #[ignore = "explicit verified model download and runtime load; set MDOC_MODEL"]
    fn settings_model_setup_probe() {
        let progress = crate::model_download::Progress::default();
        match probe_model() {
            crate::settings::Model::Ocr(model) => {
                crate::ocr::install_config(
                    &OcrConfig {
                        model,
                        ..Default::default()
                    },
                    &progress,
                )
                .unwrap();
            }
            crate::settings::Model::Pii(model) => {
                crate::pii::detector::setup_config(
                    &PiiConfig {
                        model,
                        ..Default::default()
                    },
                    &progress,
                )
                .unwrap();
            }
        }
    }
    #[test]
    #[ignore = "actual installed models, no download; set MDOC_MODEL and MDOC_REPORT; run with network denied"]
    fn settings_model_offline_probe() {
        let _permit = crate::model_work::Permit::acquire().unwrap();
        let model = probe_model();
        let mut records = Vec::new();
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-qualification");
        for language in ["english", "russian"] {
            let (input, config) = match model {
                crate::settings::Model::Ocr(model) => (
                    Input::pdf(&base.join(format!("{language}.pdf"))).unwrap(),
                    Config::Ocr(OcrConfig {
                        model,
                        ..Default::default()
                    }),
                ),
                crate::settings::Model::Pii(model) => {
                    let text = if language == "english" {
                        "Alice Morgan represents Northbridge Legal Ltd. Email alice@example.invalid. [Contact](mailto:alice@example.invalid)."
                    } else {
                        "Анна Ёлкина представляет ООО Северный мост. Телефон +7 (999) 123-45-67. Почта anna@example.invalid."
                    };
                    (
                        Input::Markdown(Arc::new(text.repeat(4))),
                        Config::Pii(PiiConfig {
                            model,
                            ..Default::default()
                        }),
                    )
                }
            };
            let hash = input.hash();
            let record = run(&input, config, None, &AtomicBool::new(false));
            assert_eq!(input.hash(), hash);
            assert!(
                record.error.is_none(),
                "{}",
                record.error.clone().unwrap_or_default()
            );
            if let crate::settings::Model::Ocr(crate::settings::OcrModel::Cyrillic) = model {
                let expected =
                    std::fs::read_to_string(base.join(format!("{language}.txt"))).unwrap();
                let actual = record.output["recognition"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|page| page["spans"].as_array().unwrap())
                    .map(|span| span["text"].as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(" ");
                assert_eq!(
                    actual.split_whitespace().collect::<Vec<_>>().join(" "),
                    expected.split_whitespace().collect::<Vec<_>>().join(" ")
                );
            }
            eprintln!(
                "{} {language}: {} ms",
                record.configuration.name(),
                record.elapsed_ms
            );
            records.push(record);
        }
        let path = std::env::var("MDOC_REPORT").expect("set MDOC_REPORT explicitly");
        save_report(Path::new(&path), &records).unwrap();
    }
    #[test]
    fn export_preserves_original_paths_and_symlink_aliases() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.md");
        std::fs::write(&source, "original").unwrap();
        let sources = vec![source.clone()];
        assert!(export_path_is_source(&source, &sources));
        assert!(!export_path_is_source(
            &dir.path().join("report.json"),
            &sources
        ));
        #[cfg(unix)]
        {
            let alias = dir.path().join("alias.md");
            std::os::unix::fs::symlink(&source, &alias).unwrap();
            assert!(export_path_is_source(&alias, &sources));
        }
        assert_eq!(std::fs::read_to_string(&source).unwrap(), "original");
    }
    #[test]
    fn frozen_input_and_ranges_are_independent_of_working_edits() {
        let mut source = "Alice@example.invalid".to_string();
        let input = Input::Markdown(Arc::new(source.clone()));
        let hash = input.hash();
        source.clear();
        assert_eq!(input.hash(), hash);
        assert_eq!(
            parse_pages("1, 3-5").unwrap().unwrap(),
            BTreeSet::from([1, 3, 4, 5])
        );
        for invalid in ["0", "5-2", "1-4294967295", "x", "1,"] {
            assert!(parse_pages(invalid).is_err());
        }
        assert!(parse_pages("").unwrap().is_none());
    }
    #[test]
    fn cancelled_run_has_no_partial_predictions_and_export_has_no_mapping() {
        let input = Input::Markdown(Arc::new("Alice".into()));
        let record = run(
            &input,
            Config::Pii(PiiConfig::default()),
            None,
            &AtomicBool::new(true),
        );
        assert!(record.output.is_null());
        assert!(record.error.is_some());
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("report.json");
        save_report(&path, &[record]).unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        assert!(!text.contains("mapping"));
        assert!(!text.contains("Alice"));
    }
}
