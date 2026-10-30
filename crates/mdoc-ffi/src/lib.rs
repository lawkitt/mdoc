//! The mobile apps' bridge to the GUI-free core (ADR 0085), through UniFFI.
//!
//! Rust owns document, review, case and restore state; Swift and Kotlin hold
//! view state only. ADR 0085's three coarse objects grow here, one module
//! each: `library` (cases, stored documents, storage paths),
//! `document_session` (import and OCR, scan, review, Copy for AI and Share
//! text) and `setup` (models that fit the device, verifying and installing
//! downloads). Until they exist, [`spike`] holds the feasibility
//! measurements and [`memory`] the footprint they report.
uniffi::setup_scaffolding!();

mod error;
pub mod memory;
pub mod spike;

pub use error::CoreError;
