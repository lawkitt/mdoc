//! GUI-free document import: conversion and its sessions, OCR and
//! its verified setup, OCR that keeps finished pages (`resume`),
//! recognition languages, model downloads and the one-at-a-time model work
//! permit, the comments the forks read, the DOCX preview (a supervised
//! worker on desktop, in process on iOS; Android has none yet) and the
//! supervised conversion worker (desktop only: the mobile apps convert
//! in process).
pub mod comments;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod conversion_worker;
mod doc_crypt;
#[cfg(not(target_os = "android"))]
pub mod docx_preview;
pub mod import;
pub mod import_session;
pub mod model_download;
pub mod model_work;
pub mod ocr;
pub mod recognition;
pub mod resume;
