//! In-memory transport shared by the core test suites.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use jifercraft_net::{ByteStream, Checksum, NetError, Transport, Url};

#[derive(Debug, Default)]
pub struct MemoryTransport {
    routes: Mutex<HashMap<String, Vec<u8>>>,
    hits: Mutex<HashMap<String, usize>>,
}

impl MemoryTransport {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn serve(&self, url: &str, body: impl Into<Vec<u8>>) {
        self.routes.lock().unwrap().insert(url.to_owned(), body.into());
    }

    pub fn hits(&self, url: &str) -> usize {
        self.hits.lock().unwrap().get(url).copied().unwrap_or(0)
    }
}

impl Transport for MemoryTransport {
    fn get(&self, url: &Url) -> impl Future<Output = Result<ByteStream, NetError>> + Send {
        *self.hits.lock().unwrap().entry(url.to_string()).or_default() += 1;
        let body = self.routes.lock().unwrap().get(url.as_str()).cloned();
        std::future::ready(match body {
            Some(body) => Ok(Box::pin(futures::stream::iter([Ok(body)])) as ByteStream),
            None => Err(NetError::Status {
                url: url.to_string(),
                status: 404,
            }),
        })
    }
}

pub fn sha1_hex(bytes: &[u8]) -> String {
    let mut hasher = Checksum::Sha1([0; 20]).hasher();
    hasher.update(bytes);
    hasher.finish().to_hex()
}
