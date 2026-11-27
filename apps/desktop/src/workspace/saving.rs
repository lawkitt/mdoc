//! How the window keeps what it holds: autosave, the writers, intake, the
//! DOCX conversion queue, the live views and the quit.
pub(super) mod autosave;
pub(super) mod document_writer;
pub(super) mod docx_queue;
pub(super) mod intake;
pub(super) mod live_views;
pub(super) mod quit_flow;
pub(super) mod session_writer;
