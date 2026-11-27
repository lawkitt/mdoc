//! Converting this tab's source into Markdown: the one import
//! job and its slot in the app-wide import permit, automatic conversion of
//! opened PDFs and DOCX files, OCR consent, recognition languages and
//! setup on the conversion card, the password card of a locked document,
//! and the tab's unlock of it. The document view reads Settings, starts the
//! background work and the model setup, and carries out the returned
//! effects.

mod actions;
mod state;
mod view;

pub(crate) use actions::known_ocr_state;
pub(crate) use state::{ImportEffect, ImportFlow, PasswordPrompt};
