//! GUI-free app storage (ADR 0069): app paths, documents and their atomic
//! saves, the document and window sessions, recents, settings, saved cases and
//! crash-report masking. Store types name no GPUI or app-style types; errors
//! are typed and become user text at the UI edge.
pub mod app_paths;
pub mod cases;
pub mod crash;
pub mod document;
pub mod document_session;
pub mod recents;
pub mod session_store;
pub mod settings;
