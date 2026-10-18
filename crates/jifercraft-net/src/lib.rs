//! HTTP transport and verified, concurrent file downloads.

pub mod checksum;
pub mod downloader;
pub mod error;
pub mod transport;

pub use checksum::Checksum;
pub use downloader::{DownloadJob, DownloadReport, DownloadSnapshot, Downloader, ProgressFn, RetryPolicy};
pub use error::NetError;
pub use transport::{ByteStream, HttpClient, Transport, fetch_bytes};
pub use url::Url;
