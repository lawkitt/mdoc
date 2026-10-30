/// Why a core call failed, in the words of the crate that failed.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("{message}")]
    Import { message: String },
    /// A PDF has pages without a text layer: import it again with OCR, or
    /// with native text only. Pages are 1-based.
    #[error("This PDF requires OCR.")]
    NeedsOcr { pages: Vec<u32>, page_count: u32 },
    #[error("{message}")]
    Ocr { message: String },
    #[error("{message}")]
    Detect { message: String },
    /// The user cancelled the job, or a newer job replaced it.
    #[error("Cancelled.")]
    Cancelled,
    /// A review transition did not apply: the text or mention changed.
    #[error("{message}")]
    Review { message: String },
    /// Reading or writing a case, or a case rule (one case per document).
    #[error("{message}")]
    Case { message: String },
    /// Verifying or installing a model file.
    #[error("{message}")]
    Setup { message: String },
    /// An argument the platform passed is not one the core accepts.
    #[error("{message}")]
    Invalid { message: String },
    #[error("{message}")]
    Io { message: String },
}

impl CoreError {
    pub(crate) fn review(message: impl Into<String>) -> Self {
        Self::Review {
            message: message.into(),
        }
    }
    pub(crate) fn case(message: impl Into<String>) -> Self {
        Self::Case {
            message: message.into(),
        }
    }
    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid {
            message: message.into(),
        }
    }
}

impl From<mdoc_import::import::ImportError> for CoreError {
    fn from(error: mdoc_import::import::ImportError) -> Self {
        use mdoc_import::import::ImportError;
        match error {
            ImportError::Cancelled => Self::Cancelled,
            ImportError::NeedsOcr(_, pages, _, page_count) => Self::NeedsOcr { pages, page_count },
            error => Self::Import {
                message: error.to_string(),
            },
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
        match error {
            mdoc_detect::DetectError::Cancelled => Self::Cancelled,
            error => Self::Detect {
                message: error.to_string(),
            },
        }
    }
}

impl From<mdoc_store::cases::CaseError> for CoreError {
    fn from(error: mdoc_store::cases::CaseError) -> Self {
        Self::case(error.to_string())
    }
}

impl From<mdoc_import::model_download::DownloadError> for CoreError {
    fn from(error: mdoc_import::model_download::DownloadError) -> Self {
        Self::Setup {
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
