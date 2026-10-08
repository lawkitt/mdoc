//! PII pseudonymization in the app: the GUI-free review model from `mdoc_pii`,
//! offline detection and the review workspace UI.
pub mod detector;
pub mod ui;
pub use mdoc_pii::*;
