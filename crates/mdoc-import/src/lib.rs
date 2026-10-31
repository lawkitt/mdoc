//! GUI-free document import (ADR 0069): conversion and its sessions, OCR and
//! its verified setup, OCR that keeps finished pages (`resume`, ADR 0085),
//! recognition languages, model downloads and the one-at-a-time model work
//! permit, and the supervised DOCX preview worker (desktop only: the mobile
//! apps convert DOCX in-process and have no preview, ADR 0085).
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod docx_preview;
pub mod import;
pub mod import_session;
pub mod model_download;
pub mod model_work;
pub mod ocr;
pub mod recognition;
pub mod resume;
