//! GUI-free document import (ADR 0069): conversion and its sessions, OCR and
//! its verified setup, OCR that keeps finished pages (`resume`, ADR 0085),
//! recognition languages, model downloads and the one-at-a-time model work
//! permit, the DOCX preview (a supervised worker on desktop, in process on
//! iOS; Android has none yet) and the supervised conversion worker (desktop
//! only: the mobile apps convert in process).
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod conversion_worker;
#[cfg(not(target_os = "android"))]
pub mod docx_preview;
pub mod hidden_content;
pub mod import;
pub mod import_session;
pub mod model_download;
pub mod model_work;
pub mod ocr;
pub mod recognition;
pub mod resume;
