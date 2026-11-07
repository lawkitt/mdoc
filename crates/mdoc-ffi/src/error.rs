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

/// Why a review call (`DocumentSession::review`, `hand_off`) did not
/// apply, typed so each shows its own message (ADR 0104).
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ReviewError {
    /// Copy for AI and Share wait until every proposal is replaced or kept.
    #[error("Review the {count} proposals left first.")]
    PendingProposals { count: u64 },
    /// Rename to an alias another identity has: Same as is the way to it.
    #[error("{alias} is another name's alias.")]
    AliasTaken { alias: String, identity: u64 },
    /// The mention or identity named is not in the review now.
    #[error("That name is no longer in the document.")]
    NotFound,
    /// The core refused the change (a retired or malformed alias, a
    /// mention that moved, no document), in its own words.
    #[error("{message}")]
    Refused { message: String },
    /// Reading or writing the document or its case failed.
    #[error("{message}")]
    Storage { message: String },
}

impl From<CoreError> for ReviewError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::Io { message } => Self::Storage { message },
            error => Self::Refused {
                message: error.to_string(),
            },
        }
    }
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
