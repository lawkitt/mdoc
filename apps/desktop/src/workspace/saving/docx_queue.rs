//! DOCX conversions waiting for the one supervised worker: tabs
//! queue their jobs, one runs at a time, and cancelled jobs or jobs of
//! closed tabs are skipped. The workspace view hands the next job to its
//! document, which runs it and reports back. Its state and actions each have their file.
mod actions;
mod state;

pub(in crate::workspace) use state::{DocxEffect, DocxJob, DocxQueue};
