//! The mobile apps' bridge to the GUI-free core, through UniFFI.
//!
//! Rust owns document, review, case and restore state; Swift and Kotlin hold
//! view state only. Three coarse objects, one module each:
//! [`library`] (cases, stored documents, storage paths, restoring replies),
//! [`document_session`] (import and OCR, scan, review, Copy for AI and Share
//! text, saving into a case) and [`setup`] (models that fit the device,
//! verifying and installing downloads). [`editor`] exports the stateless
//! editing rules edit mode calls, and [`original`] renders a DOCX
//! file for the Original pane.
uniffi::setup_scaffolding!();

pub mod document_session;
pub mod editor;
mod error;
pub mod library;
pub mod markdown;
#[cfg(not(target_os = "android"))]
pub mod original;
pub mod setup;

pub use document_session::DocumentSession;
pub use error::{CoreError, ReviewError};
pub use library::Library;
pub use setup::Setup;

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};

    /// A new, empty folder under the system temporary folder, removed when
    /// dropped. Tests never touch the app's real data.
    pub struct Scratch(PathBuf);

    impl Scratch {
        pub fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "mdoc-ffi-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
