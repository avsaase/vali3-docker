#[derive(Debug, thiserror::Error)]
pub enum ValiError {
    #[error("Bad request")]
    Request,

    #[error("File upload failed")]
    Upload,

    #[error("File hashes do not match")]
    UploadHash,

    #[error("File too large - max {}mb", *max_size as f64 / 1_000_000.)]
    Size { max_size: usize },

    #[error("Not an IGC file")]
    Format,

    #[error("Not a valid gzip file")]
    FormatGzip,

    #[error("Not a valid zip file")]
    FormatZip,

    #[error("IGC program not supported - {ccc}")]
    Unsupported { ccc: String },

    #[error("Service error")]
    Service,
}

impl ValiError {
    pub fn status(&self) -> &'static str {
        match self {
            ValiError::Request => "ERR_REQUEST",
            ValiError::Upload => "ERR_UPLOAD",
            ValiError::UploadHash => "ERR_UPLOAD_HASH",
            ValiError::Size { .. } => "ERR_SIZE",
            ValiError::Format => "ERR_FORMAT",
            ValiError::FormatGzip => "ERR_FORMAT_GZIP",
            ValiError::FormatZip => "ERR_FORMAT_ZIP",
            ValiError::Unsupported { .. } => "ERR_UNSUPPORTED",
            ValiError::Service => "ERR_SERVICE",
        }
    }
}
