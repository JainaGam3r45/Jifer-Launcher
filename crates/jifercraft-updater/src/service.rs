//! Self-update state machine: `idle → checking → available → downloading → downloaded`, plus `error`.
//!
//! Automatic checks fail silently and never clobber an update already on offer; manual checks surface
//! their error. Installing is refused while Minecraft runs, because the installer restarts the launcher.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::Serialize;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::notes::notes_to_text;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateState {
    Idle,
    Checking,
    Available,
    Downloading,
    Downloaded,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub state: UpdateState,
    pub supported: bool,
    pub current_version: String,
    pub info: Option<UpdateInfo>,
    pub progress: Option<UpdateProgress>,
    pub error: Option<String>,
    pub last_checked_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub percent: f64,
    pub transferred: u64,
    pub total: u64,
    pub bytes_per_second: u64,
}

impl UpdateProgress {
    const EMPTY: Self = Self {
        percent: 0.0,
        transferred: 0,
        total: 0,
        bytes_per_second: 0,
    };
}

/// A release offered by the feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub date: Option<String>,
    pub notes: Option<String>,
}

/// Cumulative bytes received and the total, when known.
pub type ChunkFn = Arc<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// The host-specific updater (Tauri's plugin in the desktop app, a fake in tests).
pub trait UpdateDriver: Send + Sync + 'static {
    fn supported(&self) -> bool;
    fn current_version(&self) -> String;
    fn check(&self) -> impl Future<Output = Result<Option<Release>, String>> + Send;
    fn download(&self, on_chunk: ChunkFn) -> impl Future<Output = Result<(), String>> + Send;
    /// Applies the downloaded update and restarts the app.
    fn install(&self) -> Result<(), String>;
}

pub trait GameActivity: Send + Sync + 'static {
    fn is_running(&self) -> bool;
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("there is no downloaded update to install")]
    NotReady,
    #[error("close Minecraft before installing the update")]
    BlockedRunning,
    #[error("the update could not be installed: {0}")]
    Install(String),
}

impl UpdateError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotReady => "update_not_ready",
            Self::BlockedRunning => "update_blocked_running",
            Self::Install(_) => "update_install_failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckOrigin {
    Automatic,
    Manual,
}

#[derive(Debug, Clone, Copy)]
pub struct Schedule {
    pub startup_delay: Duration,
    pub recheck_interval: Duration,
    pub progress_interval: Duration,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            startup_delay: Duration::from_secs(15),
            recheck_interval: Duration::from_hours(6),
            progress_interval: Duration::from_millis(250),
        }
    }
}

type Listener = Box<dyn Fn(&UpdateStatus) + Send + Sync>;

pub struct UpdateService<D, G> {
    driver: D,
    game: G,
    schedule: Schedule,
    now_iso: fn() -> String,
    status: Mutex<UpdateStatus>,
    listeners: Mutex<Vec<Listener>>,
    timers: Mutex<Option<CancellationToken>>,
}

impl<D, G> std::fmt::Debug for UpdateService<D, G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateService")
            .field("schedule", &self.schedule)
            .finish_non_exhaustive()
    }
}

impl<D: UpdateDriver, G: GameActivity> UpdateService<D, G> {
    pub fn new(driver: D, game: G, schedule: Schedule, now_iso: fn() -> String) -> Self {
        let status = UpdateStatus {
            state: UpdateState::Idle,
            supported: driver.supported(),
            current_version: driver.current_version(),
            info: None,
            progress: None,
            error: None,
            last_checked_at: None,
        };
        Self {
            driver,
            game,
            schedule,
            now_iso,
            status: Mutex::new(status),
            listeners: Mutex::default(),
            timers: Mutex::default(),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        self.lock_status().clone()
    }

    pub fn subscribe(&self, listener: impl Fn(&UpdateStatus) + Send + Sync + 'static) {
        self.listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Box::new(listener));
    }

    /// Checks once after the startup delay, then on every recheck interval, until [`Self::dispose`].
    pub fn schedule_checks(self: &Arc<Self>) {
        if !self.driver.supported() {
            tracing::info!("automatic updates are disabled for this build");
            return;
        }
        let token = CancellationToken::new();
        if let Some(previous) = self
            .timers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(token.clone())
        {
            previous.cancel();
        }
        let service = Arc::clone(self);
        tokio::spawn(async move {
            let mut delay = service.schedule.startup_delay;
            loop {
                tokio::select! {
                    () = tokio::time::sleep(delay) => {}
                    () = token.cancelled() => return,
                }
                service.check(CheckOrigin::Automatic).await;
                delay = service.schedule.recheck_interval;
            }
        });
    }

    pub fn dispose(&self) {
        if let Some(token) = self.timers.lock().unwrap_or_else(PoisonError::into_inner).take() {
            token.cancel();
        }
    }

    pub async fn check(&self, origin: CheckOrigin) {
        let previous = {
            let mut status = self.lock_status();
            let busy = matches!(
                status.state,
                UpdateState::Checking | UpdateState::Downloading | UpdateState::Downloaded
            );
            if !self.driver.supported() || busy {
                return;
            }
            let previous = status.clone();
            status.state = UpdateState::Checking;
            status.error = None;
            previous
        };
        self.publish();

        let result = self.driver.check().await;
        let checked_at = (self.now_iso)();
        self.update(|status| match result {
            Ok(Some(release)) => {
                status.state = UpdateState::Available;
                status.info = Some(UpdateInfo {
                    version: release.version,
                    release_date: release.date,
                    notes: release.notes.as_deref().and_then(notes_to_text),
                });
                status.progress = None;
                status.last_checked_at = Some(checked_at);
            }
            Ok(None) => {
                status.state = UpdateState::Idle;
                status.info = None;
                status.progress = None;
                status.last_checked_at = Some(checked_at);
            }
            Err(error) => {
                tracing::warn!(%error, "update check failed");
                *status = previous;
                if origin == CheckOrigin::Manual {
                    status.state = UpdateState::Error;
                    status.error = Some(error);
                }
            }
        });
    }

    pub async fn download(&self) {
        {
            let mut status = self.lock_status();
            if status.state != UpdateState::Available {
                return;
            }
            status.state = UpdateState::Downloading;
            status.progress = Some(UpdateProgress::EMPTY);
            status.error = None;
        }
        self.publish();

        let started = Instant::now();
        let last_emit: Mutex<Option<Instant>> = Mutex::new(None);
        let interval = self.schedule.progress_interval;
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<UpdateProgress>();
        let on_chunk: ChunkFn = Arc::new(move |transferred, total| {
            let now = Instant::now();
            let mut last = last_emit.lock().unwrap_or_else(PoisonError::into_inner);
            if last.is_some_and(|at| now.duration_since(at) < interval) {
                return;
            }
            *last = Some(now);
            let _ = sender.send(progress(transferred, total, now.duration_since(started)));
        });

        let download = self.driver.download(on_chunk);
        tokio::pin!(download);
        let result = loop {
            tokio::select! {
                result = &mut download => break result,
                Some(progress) = receiver.recv() => self.update(|status| {
                    if status.state == UpdateState::Downloading {
                        status.progress = Some(progress);
                    }
                }),
            }
        };
        while let Ok(progress) = receiver.try_recv() {
            self.update(|status| status.progress = Some(progress));
        }

        self.update(|status| match result {
            Ok(()) => {
                status.state = UpdateState::Downloaded;
                let mut done = status.progress.unwrap_or(UpdateProgress::EMPTY);
                done.percent = 100.0;
                status.progress = Some(done);
            }
            Err(error) => {
                tracing::warn!(%error, "update download failed");
                status.state = UpdateState::Error;
                status.progress = None;
                status.error = Some(error);
            }
        });
    }

    pub fn install(&self) -> Result<(), UpdateError> {
        if self.lock_status().state != UpdateState::Downloaded {
            return Err(UpdateError::NotReady);
        }
        if self.game.is_running() {
            return Err(UpdateError::BlockedRunning);
        }
        self.driver.install().map_err(UpdateError::Install)
    }

    fn update(&self, change: impl FnOnce(&mut UpdateStatus)) {
        change(&mut self.lock_status());
        self.publish();
    }

    fn publish(&self) {
        let snapshot = self.status();
        for listener in self.listeners.lock().unwrap_or_else(PoisonError::into_inner).iter() {
            listener(&snapshot);
        }
    }

    fn lock_status(&self) -> std::sync::MutexGuard<'_, UpdateStatus> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn progress(transferred: u64, total: Option<u64>, elapsed: Duration) -> UpdateProgress {
    let total = total.unwrap_or(0);
    let ratio = if total == 0 {
        0.0
    } else {
        to_f64(transferred) / to_f64(total)
    };
    let millis = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX).max(1);
    UpdateProgress {
        percent: (ratio * 100.0).clamp(0.0, 100.0),
        transferred,
        total,
        bytes_per_second: transferred.saturating_mul(1000) / millis,
    }
}

#[expect(clippy::cast_precision_loss, reason = "byte counts far below 2^52 are exact")]
fn to_f64(value: u64) -> f64 {
    value as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Default)]
    struct FakeDriver {
        unsupported: bool,
        checks: Mutex<VecDeque<Result<Option<Release>, String>>>,
        check_calls: AtomicUsize,
        download_result: Mutex<Option<Result<(), String>>>,
        chunks: Mutex<Vec<(u64, Duration)>>,
        installed: AtomicBool,
        check_gate: Option<Arc<tokio::sync::Notify>>,
    }

    impl UpdateDriver for Arc<FakeDriver> {
        fn supported(&self) -> bool {
            !self.unsupported
        }
        fn current_version(&self) -> String {
            "0.2.0".into()
        }
        async fn check(&self) -> Result<Option<Release>, String> {
            self.check_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(gate) = &self.check_gate {
                gate.notified().await;
            }
            self.checks.lock().unwrap().pop_front().unwrap_or(Ok(None))
        }
        async fn download(&self, on_chunk: ChunkFn) -> Result<(), String> {
            let chunks = self.chunks.lock().unwrap().clone();
            for (bytes, wait) in chunks {
                tokio::time::sleep(wait).await;
                on_chunk(bytes, Some(1000));
            }
            self.download_result.lock().unwrap().take().unwrap_or(Ok(()))
        }
        fn install(&self) -> Result<(), String> {
            self.installed.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    struct Game(AtomicBool);

    impl GameActivity for Arc<Game> {
        fn is_running(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
    }

    type Service = UpdateService<Arc<FakeDriver>, Arc<Game>>;

    fn release(version: &str) -> Release {
        Release {
            version: version.into(),
            date: Some("2026-11-01".into()),
            notes: Some("<p>Faster &amp; safer</p>".into()),
        }
    }

    fn service(driver: &Arc<FakeDriver>) -> (Arc<Service>, Arc<Game>, Arc<Mutex<Vec<UpdateStatus>>>) {
        let game = Arc::new(Game(AtomicBool::new(false)));
        let service = Arc::new(UpdateService::new(
            Arc::clone(driver),
            Arc::clone(&game),
            Schedule::default(),
            || "2026-11-10T10:00:00.000Z".into(),
        ));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        service.subscribe(move |status| sink.lock().unwrap().push(status.clone()));
        (service, game, seen)
    }

    fn driver_with(checks: Vec<Result<Option<Release>, String>>) -> Arc<FakeDriver> {
        Arc::new(FakeDriver {
            checks: Mutex::new(checks.into()),
            ..FakeDriver::default()
        })
    }

    #[tokio::test]
    async fn starts_idle_with_the_driver_version() {
        let (service, _, _) = service(&driver_with(vec![]));
        let status = service.status();
        assert_eq!(
            (status.state, status.supported, status.current_version.as_str()),
            (UpdateState::Idle, true, "0.2.0")
        );
    }

    #[tokio::test]
    async fn offers_an_update_with_plain_text_notes() {
        let (service, _, _) = service(&driver_with(vec![Ok(Some(release("0.3.0")))]));
        service.check(CheckOrigin::Manual).await;
        let status = service.status();
        assert_eq!(status.state, UpdateState::Available);
        let info = status.info.unwrap();
        assert_eq!(
            (info.version.as_str(), info.notes.as_deref()),
            ("0.3.0", Some("Faster & safer"))
        );
        assert_eq!(status.last_checked_at.as_deref(), Some("2026-11-10T10:00:00.000Z"));
    }

    #[tokio::test]
    async fn nothing_new_returns_to_idle_and_records_the_check() {
        let (service, _, seen) = service(&driver_with(vec![Ok(None)]));
        service.check(CheckOrigin::Manual).await;
        let states: Vec<_> = seen.lock().unwrap().iter().map(|s| s.state).collect();
        assert_eq!(states, [UpdateState::Checking, UpdateState::Idle]);
        assert!(service.status().last_checked_at.is_some());
    }

    #[tokio::test]
    async fn manual_failures_surface_but_automatic_ones_stay_silent() {
        let (service, _, _) = service(&driver_with(vec![Err("offline".into()), Err("offline".into())]));
        service.check(CheckOrigin::Automatic).await;
        assert_eq!(service.status().state, UpdateState::Idle);
        service.check(CheckOrigin::Manual).await;
        let status = service.status();
        assert_eq!(
            (status.state, status.error.as_deref()),
            (UpdateState::Error, Some("offline"))
        );
    }

    #[tokio::test]
    async fn a_failed_automatic_check_keeps_an_offered_update() {
        let (service, _, _) = service(&driver_with(vec![Ok(Some(release("0.3.0"))), Err("offline".into())]));
        service.check(CheckOrigin::Manual).await;
        service.check(CheckOrigin::Automatic).await;
        let status = service.status();
        assert_eq!(status.state, UpdateState::Available);
        assert_eq!(status.info.unwrap().version, "0.3.0");
    }

    #[tokio::test]
    async fn concurrent_checks_collapse_into_one() {
        let gate = Arc::new(tokio::sync::Notify::new());
        let driver = Arc::new(FakeDriver {
            check_gate: Some(Arc::clone(&gate)),
            ..FakeDriver::default()
        });
        let (service, _, _) = service(&driver);
        let first = tokio::spawn({
            let service = Arc::clone(&service);
            async move { service.check(CheckOrigin::Manual).await }
        });
        tokio::task::yield_now().await;
        service.check(CheckOrigin::Manual).await;
        gate.notify_one();
        first.await.unwrap();
        assert_eq!(driver.check_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn unsupported_builds_never_check() {
        let driver = Arc::new(FakeDriver {
            unsupported: true,
            ..FakeDriver::default()
        });
        let (service, _, seen) = service(&driver);
        service.check(CheckOrigin::Manual).await;
        service.schedule_checks();
        assert_eq!(driver.check_calls.load(Ordering::SeqCst), 0);
        assert_eq!(seen.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn download_only_starts_from_available() {
        let driver = driver_with(vec![]);
        let (service, _, seen) = service(&driver);
        service.download().await;
        assert_eq!(seen.lock().unwrap().len(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn downloads_with_throttled_progress() {
        let driver = driver_with(vec![Ok(Some(release("0.3.0")))]);
        *driver.chunks.lock().unwrap() = vec![
            (100, Duration::from_millis(10)),
            (200, Duration::from_millis(10)),
            (300, Duration::from_millis(10)),
            (600, Duration::from_millis(300)),
            (1000, Duration::from_millis(300)),
        ];
        let (service, _, seen) = service(&driver);
        service.check(CheckOrigin::Manual).await;
        service.download().await;

        let progress: Vec<u64> = seen
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.state == UpdateState::Downloading)
            .filter_map(|s| s.progress.map(|p| p.transferred))
            .collect();
        assert_eq!(
            progress,
            [0, 100, 600, 1000],
            "chunks inside the 250 ms window are dropped"
        );
        let status = service.status();
        assert_eq!(status.state, UpdateState::Downloaded);
        assert!((status.progress.unwrap().percent - 100.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn a_failed_download_reports_the_error() {
        let driver = driver_with(vec![Ok(Some(release("0.3.0")))]);
        *driver.download_result.lock().unwrap() = Some(Err("disk full".into()));
        let (service, _, _) = service(&driver);
        service.check(CheckOrigin::Manual).await;
        service.download().await;
        let status = service.status();
        assert_eq!(
            (status.state, status.error.as_deref(), status.progress),
            (UpdateState::Error, Some("disk full"), None)
        );
    }

    #[tokio::test]
    async fn install_requires_a_download_and_no_running_game() {
        let driver = driver_with(vec![Ok(Some(release("0.3.0")))]);
        let (service, game, _) = service(&driver);
        assert_eq!(service.install().unwrap_err().code(), "update_not_ready");
        service.check(CheckOrigin::Manual).await;
        service.download().await;
        game.0.store(true, Ordering::SeqCst);
        assert_eq!(service.install().unwrap_err().code(), "update_blocked_running");
        game.0.store(false, Ordering::SeqCst);
        service.install().unwrap();
        assert!(driver.installed.load(Ordering::SeqCst));
    }

    #[tokio::test(start_paused = true)]
    async fn schedules_a_startup_check_then_rechecks_until_disposed() {
        let driver = driver_with(vec![]);
        let (service, _, _) = service(&driver);
        service.schedule_checks();
        tokio::time::sleep(Duration::from_secs(14)).await;
        assert_eq!(driver.check_calls.load(Ordering::SeqCst), 0);
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(driver.check_calls.load(Ordering::SeqCst), 1);
        tokio::time::sleep(Duration::from_hours(6)).await;
        assert_eq!(driver.check_calls.load(Ordering::SeqCst), 2);
        service.dispose();
        tokio::time::sleep(Duration::from_hours(12)).await;
        assert_eq!(driver.check_calls.load(Ordering::SeqCst), 2);
    }
}
