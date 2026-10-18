use std::pin::Pin;
use std::time::Duration;

use futures::{Stream, StreamExt, TryStreamExt};
use url::Url;

use crate::error::NetError;

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Vec<u8>, NetError>> + Send>>;

/// The single seam between the downloader and the network, so tests can serve bytes from memory.
pub trait Transport: Send + Sync + 'static {
    fn get(&self, url: &Url) -> impl Future<Output = Result<ByteStream, NetError>> + Send;
}

/// Reads a whole (small) response into memory, refusing anything larger than `limit`.
pub async fn fetch_bytes<T: Transport>(transport: &T, url: &Url, limit: u64) -> Result<Vec<u8>, NetError> {
    let mut stream = transport.get(url).await?;
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        body.extend_from_slice(&chunk?);
        if body.len() as u64 > limit {
            return Err(NetError::TooLarge {
                url: url.to_string(),
                limit,
            });
        }
    }
    Ok(body)
}

/// Production transport over reqwest (rustls, system proxy).
#[derive(Debug, Clone)]
pub struct HttpClient {
    client: reqwest::Client,
}

impl HttpClient {
    pub fn new(user_agent: &str) -> Result<Self, NetError> {
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            .build()
            .map_err(NetError::Client)?;
        Ok(Self { client })
    }

    /// The underlying client, for request shapes the downloader does not need (form posts, bearer GETs).
    pub fn inner(&self) -> &reqwest::Client {
        &self.client
    }
}

impl Transport for HttpClient {
    async fn get(&self, url: &Url) -> Result<ByteStream, NetError> {
        let response = self
            .client
            .get(url.clone())
            .send()
            .await
            .map_err(|source| NetError::Request {
                url: url.to_string(),
                source,
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(NetError::Status {
                url: url.to_string(),
                status: status.as_u16(),
            });
        }
        let url = url.clone();
        Ok(Box::pin(
            response
                .bytes_stream()
                .map_ok(|chunk| chunk.to_vec())
                .map_err(move |source| NetError::Request {
                    url: url.to_string(),
                    source,
                }),
        ))
    }
}
