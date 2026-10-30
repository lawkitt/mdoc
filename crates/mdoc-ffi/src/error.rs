/// Why a core call failed, in the words of the crate that failed.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("{message}")]
    Import { message: String },
    #[error("{message}")]
    Ocr { message: String },
    #[error("{message}")]
    Detect { message: String },
    #[error("{message}")]
    Io { message: String },
}

impl From<mdoc_import::import::ImportError> for CoreError {
    fn from(error: mdoc_import::import::ImportError) -> Self {
        Self::Import {
            message: error.to_string(),
        }
    }
}

impl From<mdoc_import::ocr::SetupError> for CoreError {
    fn from(error: mdoc_import::ocr::SetupError) -> Self {
        Self::Ocr {
            message: error.to_string(),
        }
    }
}

impl From<mdoc_detect::DetectError> for CoreError {
    fn from(error: mdoc_detect::DetectError) -> Self {
        Self::Detect {
            message: error.to_string(),
        }
    }
}

impl From<std::io::Error> for CoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io {
            message: error.to_string(),
        }
    }
}
