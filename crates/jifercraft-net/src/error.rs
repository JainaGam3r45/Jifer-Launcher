use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("could not initialise the HTTP client: {0}")]
    Client(#[source] reqwest::Error),
    #[error("request to {url} failed: {source}")]
    Request {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("{url} answered HTTP {status}")]
    Status { url: String, status: u16 },
    #[error("checksum mismatch for {url}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        url: String,
        expected: String,
        actual: String,
    },
    #[error("size mismatch for {url}: expected {expected} bytes, got {actual}")]
    SizeMismatch { url: String, expected: u64, actual: u64 },
    #[error("response from {url} exceeds {limit} bytes")]
    TooLarge { url: String, limit: u64 },
    #[error("invalid checksum literal \"{0}\"")]
    InvalidChecksum(String),
    #[error("i/o error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("download cancelled")]
    Cancelled,
    #[error("download worker failed: {0}")]
    Worker(String),
}

impl NetError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Client(_) => "http_client_unavailable",
            Self::Request { .. } => "network_unreachable",
            Self::Status { .. } => "http_status",
            Self::ChecksumMismatch { .. } => "checksum_mismatch",
            Self::SizeMismatch { .. } => "size_mismatch",
            Self::TooLarge { .. } => "response_too_large",
            Self::InvalidChecksum(_) => "invalid_checksum",
            Self::Io { .. } => "io_error",
            Self::Cancelled => "cancelled",
            Self::Worker(_) => "download_worker_failed",
        }
    }

    /// Transient failures worth another attempt. Client errors and local disk errors are not,
    /// except when Windows Defender or the indexer momentarily locks a file right during rename.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Request { .. } | Self::ChecksumMismatch { .. } | Self::SizeMismatch { .. } => true,
            Self::Status { status, .. } => *status == 408 || *status == 429 || *status >= 500,
            Self::Io { source, .. } => {
                #[cfg(windows)]
                {
                    // Si el antivirus nos metió el pie en el disco justo al renombrar
                    // (32: SHARING_VIOLATION, 5: ACCESS_DENIED, 33: LOCK_VIOLATION, 1224: USER_MAPPED_FILE)
                    matches!(source.raw_os_error(), Some(32 | 5 | 33 | 1224))
                }
                #[cfg(not(windows))]
                {
                    let _ = source;
                    false
                }
            }
            _ => false,
        }
    }

    pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
