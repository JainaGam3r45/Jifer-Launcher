//! Bounded-concurrency downloads where nothing reaches its final path unverified.
//!
//! Each file streams into `<dest>.part` while being hashed, is size- and checksum-checked, flushed to disk
//! and only then renamed into place. Files already on disk with a matching checksum are reused untouched.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use futures::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::checksum::Checksum;
use crate::error::NetError;
use crate::transport::Transport;

#[derive(Debug, Clone)]
pub struct DownloadJob {
    pub url: Url,
    pub dest: PathBuf,
    pub size: Option<u64>,
    pub checksum: Option<Checksum>,
}

/// Point-in-time telemetry for the UI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DownloadSnapshot {
    pub files_total: u64,
    pub files_done: u64,
    pub files_reused: u64,
    pub files_active: u64,
    pub files_verifying: u64,
    pub files_verified: u64,
    pub retries: u64,
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub bytes_per_second: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DownloadReport {
    pub downloaded: u64,
    pub reused: u64,
    pub bytes_transferred: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Extra attempts after the first one.
    pub retries: u32,
    pub base_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            retries: 3,
            base_delay: Duration::from_millis(500),
        }
    }
}

pub type ProgressFn = Arc<dyn Fn(DownloadSnapshot) + Send + Sync>;

#[derive(Debug)]
pub struct Downloader<T> {
    transport: Arc<T>,
    concurrency: usize,
    retry: RetryPolicy,
    tick: Duration,
}

impl<T: Transport> Downloader<T> {
    pub fn new(transport: Arc<T>) -> Self {
        Self {
            transport,
            concurrency: 16,
            retry: RetryPolicy::default(),
            tick: Duration::from_millis(100),
        }
    }

    #[must_use]
    pub fn with_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency.max(1);
        self
    }

    #[must_use]
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Runs every job; the first unrecoverable failure cancels the rest and is returned.
    pub async fn run(
        &self,
        jobs: Vec<DownloadJob>,
        cancel: &CancellationToken,
        on_progress: ProgressFn,
    ) -> Result<DownloadReport, NetError> {
        let jobs = dedupe(jobs);
        let counters = Arc::new(Counters::for_jobs(&jobs));
        let workers_token = cancel.child_token();
        let semaphore = Arc::new(Semaphore::new(self.concurrency));
        let mut workers = JoinSet::new();

        for job in jobs {
            let context = Worker {
                transport: Arc::clone(&self.transport),
                counters: Arc::clone(&counters),
                token: workers_token.clone(),
                retry: self.retry,
            };
            let semaphore = Arc::clone(&semaphore);
            workers.spawn(async move {
                let _permit = tokio::select! {
                    permit = semaphore.acquire_owned() => permit.map_err(|_| NetError::Cancelled)?,
                    () = context.token.cancelled() => return Err(NetError::Cancelled),
                };
                context.process(job).await
            });
        }

        let ticker_stop = CancellationToken::new();
        let ticker = tokio::spawn(report_progress(
            Arc::clone(&counters),
            Arc::clone(&on_progress),
            self.tick,
            ticker_stop.clone(),
        ));

        let mut first_error = None;
        while let Some(joined) = workers.join_next().await {
            let outcome = joined.unwrap_or_else(|error| Err(NetError::Worker(error.to_string())));
            if let Err(error) = outcome
                && first_error.is_none()
            {
                workers_token.cancel();
                first_error = Some(error);
            }
        }

        ticker_stop.cancel();
        let _ = ticker.await;
        on_progress(counters.snapshot());

        if cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(counters.report()),
        }
    }
}

struct Worker<T> {
    transport: Arc<T>,
    counters: Arc<Counters>,
    token: CancellationToken,
    retry: RetryPolicy,
}

impl<T: Transport> Worker<T> {
    async fn process(&self, job: DownloadJob) -> Result<(), NetError> {
        let _active = Gauge::enter(&self.counters.files_active);
        if self.is_reusable(&job).await? {
            let size = match job.size {
                Some(size) => size,
                None => tokio::fs::metadata(&job.dest).await.map_or(0, |m| m.len()),
            };
            self.counters.bytes_done.fetch_add(size, Ordering::Relaxed);
            self.counters.files_reused.fetch_add(1, Ordering::Relaxed);
            self.counters.files_done.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }

        let mut attempt = 0;
        loop {
            let mut in_flight = 0;
            let result = self.attempt(&job, &mut in_flight).await;
            if result.is_err() {
                self.counters.bytes_done.fetch_sub(in_flight, Ordering::Relaxed);
                let _ = tokio::fs::remove_file(part_path(&job.dest)).await;
            }
            match result {
                Ok(()) => {
                    self.counters.files_done.fetch_add(1, Ordering::Relaxed);
                    return Ok(());
                }
                Err(error) if error.is_retryable() && attempt < self.retry.retries && !self.token.is_cancelled() => {
                    attempt += 1;
                    self.counters.retries.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(url = %job.url, attempt, %error, "retrying download");
                    let delay = self.retry.base_delay.saturating_mul(1 << attempt.min(6));
                    tokio::select! {
                        () = tokio::time::sleep(delay) => {}
                        () = self.token.cancelled() => return Err(NetError::Cancelled),
                    }
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn is_reusable(&self, job: &DownloadJob) -> Result<bool, NetError> {
        let Ok(metadata) = tokio::fs::metadata(&job.dest).await else {
            return Ok(false);
        };
        if job.size.is_some_and(|size| size != metadata.len()) {
            return Ok(false);
        }
        let Some(expected) = job.checksum else { return Ok(true) };
        let _verifying = Gauge::enter(&self.counters.files_verifying);
        let dest = job.dest.clone();
        let matches = tokio::task::spawn_blocking(move || expected.matches_file(&dest))
            .await
            .map_err(|error| NetError::Worker(error.to_string()))?
            .unwrap_or(false);
        if matches {
            self.counters.files_verified.fetch_add(1, Ordering::Relaxed);
        }
        Ok(matches)
    }

    async fn attempt(&self, job: &DownloadJob, in_flight: &mut u64) -> Result<(), NetError> {
        let part = part_path(&job.dest);
        if let Some(parent) = job.dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| NetError::io(parent, error))?;
        }
        let mut stream = self.transport.get(&job.url).await?;
        let mut file = tokio::fs::File::create(&part)
            .await
            .map_err(|error| NetError::io(&part, error))?;
        let mut hasher = job.checksum.map(|checksum| checksum.hasher());

        loop {
            let next = tokio::select! {
                biased;
                () = self.token.cancelled() => return Err(NetError::Cancelled),
                next = stream.next() => next,
            };
            let Some(chunk) = next else { break };
            let chunk = chunk?;
            let len = chunk.len() as u64;
            if let Some(hasher) = hasher.as_mut() {
                hasher.update(&chunk);
            }
            file.write_all(&chunk)
                .await
                .map_err(|error| NetError::io(&part, error))?;
            *in_flight += len;
            self.counters.bytes_done.fetch_add(len, Ordering::Relaxed);
            self.counters.bytes_transferred.fetch_add(len, Ordering::Relaxed);
            if let Some(expected) = job.size
                && *in_flight > expected
            {
                return Err(NetError::SizeMismatch {
                    url: job.url.to_string(),
                    expected,
                    actual: *in_flight,
                });
            }
        }
        file.flush().await.map_err(|error| NetError::io(&part, error))?;
        file.sync_all().await.map_err(|error| NetError::io(&part, error))?;
        drop(file);

        if let Some(expected) = job.size
            && *in_flight != expected
        {
            return Err(NetError::SizeMismatch {
                url: job.url.to_string(),
                expected,
                actual: *in_flight,
            });
        }
        if let (Some(expected), Some(hasher)) = (job.checksum, hasher) {
            let actual = hasher.finish();
            if actual != expected {
                return Err(NetError::ChecksumMismatch {
                    url: job.url.to_string(),
                    expected: expected.to_hex(),
                    actual: actual.to_hex(),
                });
            }
            self.counters.files_verified.fetch_add(1, Ordering::Relaxed);
        }
        rename_part(&part, &job.dest).await
    }
}

/// Windows Defender adora examinar los archivos .part recién creados antes de que
/// podamos renombrarlos a su ruta final. Si nos bloquea el archivo momentáneamente,
/// esperamos unos milisegundos y reintentamos en vez de abortar la descarga del juego.
async fn rename_part(part: &Path, dest: &Path) -> Result<(), NetError> {
    let mut attempts = 0;
    loop {
        match tokio::fs::rename(part, dest).await {
            Ok(()) => return Ok(()),
            Err(error) => {
                let is_transient = is_transient_io(&error);
                if is_transient && attempts < 5 {
                    attempts += 1;
                    tokio::time::sleep(Duration::from_millis(20 * (1 << attempts))).await;
                    continue;
                }
                return Err(NetError::io(dest, error));
            }
        }
    }
}

fn is_transient_io(error: &std::io::Error) -> bool {
    #[cfg(windows)]
    {
        // 32: ERROR_SHARING_VIOLATION, 5: ERROR_ACCESS_DENIED, 33: ERROR_LOCK_VIOLATION, 1224: ERROR_USER_MAPPED_FILE
        matches!(error.raw_os_error(), Some(32 | 5 | 33 | 1224))
    }
    #[cfg(not(windows))]
    {
        let _ = error;
        false
    }
}

#[derive(Debug, Default)]
struct Counters {
    files_total: u64,
    bytes_total: u64,
    files_done: AtomicU64,
    files_reused: AtomicU64,
    files_active: AtomicU64,
    files_verifying: AtomicU64,
    files_verified: AtomicU64,
    retries: AtomicU64,
    bytes_done: AtomicU64,
    bytes_transferred: AtomicU64,
    bytes_per_second: AtomicU64,
}

impl Counters {
    fn for_jobs(jobs: &[DownloadJob]) -> Self {
        Self {
            files_total: jobs.len() as u64,
            bytes_total: jobs.iter().filter_map(|job| job.size).sum(),
            ..Self::default()
        }
    }

    fn snapshot(&self) -> DownloadSnapshot {
        let load = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        DownloadSnapshot {
            files_total: self.files_total,
            files_done: load(&self.files_done),
            files_reused: load(&self.files_reused),
            files_active: load(&self.files_active),
            files_verifying: load(&self.files_verifying),
            files_verified: load(&self.files_verified),
            retries: load(&self.retries),
            bytes_total: self.bytes_total,
            bytes_done: load(&self.bytes_done),
            bytes_per_second: load(&self.bytes_per_second),
        }
    }

    fn report(&self) -> DownloadReport {
        let reused = self.files_reused.load(Ordering::Relaxed);
        DownloadReport {
            downloaded: self.files_done.load(Ordering::Relaxed).saturating_sub(reused),
            reused,
            bytes_transferred: self.bytes_transferred.load(Ordering::Relaxed),
        }
    }
}

/// Increments a gauge for the lifetime of the guard.
struct Gauge<'a>(&'a AtomicU64);

impl<'a> Gauge<'a> {
    fn enter(counter: &'a AtomicU64) -> Self {
        counter.fetch_add(1, Ordering::Relaxed);
        Self(counter)
    }
}

impl Drop for Gauge<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Emits snapshots on a fixed cadence instead of per chunk, smoothing throughput with an EMA.
async fn report_progress(counters: Arc<Counters>, on_progress: ProgressFn, tick: Duration, stop: CancellationToken) {
    let mut interval = tokio::time::interval(tick);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_at = Instant::now();
    let mut last_bytes = 0_u64;
    let mut smoothed = 0_u64;
    loop {
        tokio::select! {
            () = stop.cancelled() => return,
            _ = interval.tick() => {}
        }
        let now = Instant::now();
        let bytes = counters.bytes_transferred.load(Ordering::Relaxed);
        let elapsed_ms = u64::try_from(now.duration_since(last_at).as_millis())
            .unwrap_or(u64::MAX)
            .max(1);
        let rate = bytes.saturating_sub(last_bytes).saturating_mul(1000) / elapsed_ms;
        smoothed = if smoothed == 0 {
            rate
        } else {
            (rate * 3 + smoothed * 7) / 10
        };
        counters.bytes_per_second.store(smoothed, Ordering::Relaxed);
        on_progress(counters.snapshot());
        last_at = now;
        last_bytes = bytes;
    }
}

fn dedupe(jobs: Vec<DownloadJob>) -> Vec<DownloadJob> {
    let mut seen = HashSet::new();
    jobs.into_iter().filter(|job| seen.insert(job.dest.clone())).collect()
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().map(std::ffi::OsStr::to_os_string).unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::ByteStream;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Clone)]
    enum Route {
        Body(Vec<u8>),
        CorruptOnce(Vec<u8>),
        AlwaysCorrupt(Vec<u8>),
        Status(u16),
        Hang,
    }

    #[derive(Default)]
    struct FakeTransport {
        routes: HashMap<String, Route>,
        hits: Mutex<HashMap<String, usize>>,
    }

    impl FakeTransport {
        fn with(routes: &[(&str, Route)]) -> Arc<Self> {
            Arc::new(Self {
                routes: routes
                    .iter()
                    .map(|(url, route)| ((*url).to_owned(), route.clone()))
                    .collect(),
                hits: Mutex::default(),
            })
        }

        fn hits(&self, url: &str) -> usize {
            self.hits.lock().unwrap().get(url).copied().unwrap_or(0)
        }
    }

    fn chunks(body: &[u8]) -> ByteStream {
        let split = body.len() / 2;
        let (head, tail) = body.split_at(split);
        Box::pin(futures::stream::iter(vec![Ok(head.to_vec()), Ok(tail.to_vec())]))
    }

    fn corrupt(mut body: Vec<u8>) -> Vec<u8> {
        if let Some(first) = body.first_mut() {
            *first ^= 0xff;
        }
        body
    }

    impl Transport for FakeTransport {
        fn get(&self, url: &Url) -> impl Future<Output = Result<ByteStream, NetError>> + Send {
            let hit = {
                let mut hits = self.hits.lock().unwrap();
                let entry = hits.entry(url.to_string()).or_default();
                *entry += 1;
                *entry
            };
            std::future::ready(match self.routes.get(url.as_str()).cloned() {
                Some(Route::Body(body)) => Ok(chunks(&body)),
                Some(Route::CorruptOnce(body)) => Ok(chunks(&if hit == 1 { corrupt(body) } else { body })),
                Some(Route::AlwaysCorrupt(body)) => Ok(chunks(&corrupt(body))),
                Some(Route::Status(status)) => Err(NetError::Status {
                    url: url.to_string(),
                    status,
                }),
                Some(Route::Hang) => Ok(Box::pin(futures::stream::pending()) as ByteStream),
                None => Err(NetError::Status {
                    url: url.to_string(),
                    status: 404,
                }),
            })
        }
    }

    fn job(dir: &Path, name: &str, body: &[u8]) -> DownloadJob {
        let mut hasher = Checksum::Sha1([0; 20]).hasher();
        hasher.update(body);
        DownloadJob {
            url: Url::parse(&format!("https://cdn.test/{name}")).unwrap(),
            dest: dir.join("nested").join(name),
            size: Some(body.len() as u64),
            checksum: Some(hasher.finish()),
        }
    }

    fn downloader(transport: Arc<FakeTransport>) -> Downloader<FakeTransport> {
        Downloader::new(transport).with_concurrency(4).with_retry(RetryPolicy {
            retries: 2,
            base_delay: Duration::ZERO,
        })
    }

    fn recorder() -> (ProgressFn, Arc<Mutex<Vec<DownloadSnapshot>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        (Arc::new(move |snapshot| sink.lock().unwrap().push(snapshot)), seen)
    }

    fn no_part_files(dir: &Path) -> bool {
        match std::fs::read_dir(dir.join("nested")) {
            Ok(entries) => entries
                .flatten()
                .all(|e| e.path().extension().is_none_or(|ext| ext != "part")),
            Err(_) => true,
        }
    }

    #[tokio::test]
    async fn downloads_and_verifies_every_file() {
        let dir = tempfile::tempdir().unwrap();
        let bodies: Vec<(String, Vec<u8>)> = (0..10)
            .map(|n| (format!("f{n}"), vec![n; 1000 + usize::from(n)]))
            .collect();
        let routes: Vec<_> = bodies
            .iter()
            .map(|(name, body)| (format!("https://cdn.test/{name}"), Route::Body(body.clone())))
            .collect();
        let routes: Vec<(&str, Route)> = routes
            .iter()
            .map(|(url, route)| (url.as_str(), route.clone()))
            .collect();
        let transport = FakeTransport::with(&routes);
        let mut jobs: Vec<_> = bodies.iter().map(|(name, body)| job(dir.path(), name, body)).collect();
        jobs.push(jobs[0].clone());
        let (progress, seen) = recorder();

        let report = downloader(Arc::clone(&transport))
            .run(jobs, &CancellationToken::new(), progress)
            .await
            .unwrap();

        assert_eq!(report.downloaded, 10);
        assert_eq!(report.reused, 0);
        for (name, body) in &bodies {
            assert_eq!(&std::fs::read(dir.path().join("nested").join(name)).unwrap(), body);
        }
        assert_eq!(
            transport.hits("https://cdn.test/f0"),
            1,
            "duplicate destinations are fetched once"
        );
        let last = *seen.lock().unwrap().last().unwrap();
        assert_eq!((last.files_done, last.files_verified, last.files_active), (10, 10, 0));
        assert_eq!(last.bytes_done, last.bytes_total);
        assert!(no_part_files(dir.path()));
    }

    #[tokio::test]
    async fn reuses_valid_files_and_replaces_corrupt_ones() {
        let dir = tempfile::tempdir().unwrap();
        let (good, bad) = (b"good file".to_vec(), b"bad file".to_vec());
        let transport = FakeTransport::with(&[
            ("https://cdn.test/good", Route::Body(good.clone())),
            ("https://cdn.test/bad", Route::Body(bad.clone())),
        ]);
        let jobs = vec![job(dir.path(), "good", &good), job(dir.path(), "bad", &bad)];
        std::fs::create_dir_all(dir.path().join("nested")).unwrap();
        std::fs::write(dir.path().join("nested/good"), &good).unwrap();
        std::fs::write(dir.path().join("nested/bad"), b"tampered").unwrap();
        let (progress, _) = recorder();

        let report = downloader(Arc::clone(&transport))
            .run(jobs, &CancellationToken::new(), progress)
            .await
            .unwrap();

        assert_eq!((report.downloaded, report.reused), (1, 1));
        assert_eq!(transport.hits("https://cdn.test/good"), 0);
        assert_eq!(std::fs::read(dir.path().join("nested/bad")).unwrap(), bad);
    }

    #[tokio::test]
    async fn retries_a_corrupted_transfer() {
        let dir = tempfile::tempdir().unwrap();
        let body = b"flaky".to_vec();
        let transport = FakeTransport::with(&[("https://cdn.test/flaky", Route::CorruptOnce(body.clone()))]);
        let (progress, seen) = recorder();

        downloader(Arc::clone(&transport))
            .run(
                vec![job(dir.path(), "flaky", &body)],
                &CancellationToken::new(),
                progress,
            )
            .await
            .unwrap();

        assert_eq!(transport.hits("https://cdn.test/flaky"), 2);
        assert_eq!(seen.lock().unwrap().last().unwrap().retries, 1);
        assert_eq!(std::fs::read(dir.path().join("nested/flaky")).unwrap(), body);
    }

    #[tokio::test]
    async fn persistent_corruption_never_reaches_the_destination() {
        let dir = tempfile::tempdir().unwrap();
        let body = b"evil".to_vec();
        let transport = FakeTransport::with(&[("https://cdn.test/evil", Route::AlwaysCorrupt(body.clone()))]);
        let (progress, _) = recorder();

        let error = downloader(Arc::clone(&transport))
            .run(
                vec![job(dir.path(), "evil", &body)],
                &CancellationToken::new(),
                progress,
            )
            .await
            .unwrap_err();

        assert_eq!(error.code(), "checksum_mismatch");
        assert_eq!(transport.hits("https://cdn.test/evil"), 3);
        assert!(!dir.path().join("nested/evil").exists());
        assert!(no_part_files(dir.path()));
    }

    #[tokio::test]
    async fn client_errors_are_not_retried_and_stop_the_batch() {
        let dir = tempfile::tempdir().unwrap();
        let transport = FakeTransport::with(&[("https://cdn.test/gone", Route::Status(404))]);
        let (progress, _) = recorder();

        let error = downloader(Arc::clone(&transport))
            .run(vec![job(dir.path(), "gone", b"x")], &CancellationToken::new(), progress)
            .await
            .unwrap_err();

        assert_eq!(error.code(), "http_status");
        assert_eq!(transport.hits("https://cdn.test/gone"), 1);
    }

    #[tokio::test]
    async fn cancellation_stops_in_flight_transfers() {
        let dir = tempfile::tempdir().unwrap();
        let transport = FakeTransport::with(&[("https://cdn.test/slow", Route::Hang)]);
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            trigger.cancel();
        });
        let (progress, _) = recorder();

        let error = downloader(transport)
            .run(vec![job(dir.path(), "slow", b"never")], &cancel, progress)
            .await
            .unwrap_err();

        assert_eq!(error.code(), "cancelled");
        assert!(!dir.path().join("nested/slow").exists());
        assert!(no_part_files(dir.path()));
    }
}
