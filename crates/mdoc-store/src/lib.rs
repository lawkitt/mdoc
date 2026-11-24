//! GUI-free app storage: app paths, documents and their ids, the document
//! and window sessions, settings, the OS-held key, the user's lists and
//! crash-report masking. Store types name no GPUI or app-style types; errors
//! are typed and become user text at the UI edge.
pub mod app_paths;
pub mod crash;
pub mod document;
pub mod document_id;
pub mod document_session;
pub mod lists;
pub mod model_state;
pub mod os_key;
pub mod session_store;
pub mod settings;
