// Unified diagnostics hub: intercepts both internal launcher tracing and Minecraft's stdout.
// Stored in a circular buffer so opening the console late doesn't miss early startup fireworks.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use jifercraft_core::process::{GameLogLine, LogLevel};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

pub const LOG_EVENT: &str = "logs:batch";
const RING_CAPACITY: usize = 2000;
const FLUSH_EVERY: Duration = Duration::from_millis(75);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogSource {
    Launcher,
    Game,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: u64,
    pub timestamp: String,
    pub level: LogLevel,
    pub source: LogSource,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct LogHub {
    next_id: AtomicU64,
    ring: Mutex<VecDeque<LogEntry>>,
    pending: Mutex<Vec<LogEntry>>,
}

impl LogHub {
    pub fn push(&self, level: LogLevel, source: LogSource, message: String) {
        let entry = LogEntry {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            timestamp: jifercraft_core::timestamp::now_iso(),
            level,
            source,
            message,
        };
        {
            let mut ring = self.ring.lock().unwrap_or_else(PoisonError::into_inner);
            if ring.len() == RING_CAPACITY {
                ring.pop_front();
            }
            ring.push_back(entry.clone());
        }
        self.pending.lock().unwrap_or_else(PoisonError::into_inner).push(entry);
    }

    pub fn push_game(&self, line: GameLogLine) {
        self.push(line.level, LogSource::Game, line.text);
    }

    pub fn recent(&self, limit: usize) -> Vec<LogEntry> {
        let ring = self.ring.lock().unwrap_or_else(PoisonError::into_inner);
        ring.iter().skip(ring.len().saturating_sub(limit)).cloned().collect()
    }

    // Modpacks and Forge love vomiting 5,000 log lines per second during registry loading.
    // If we emit an IPC event per single line, Tauri's IPC bridge will freeze the webview.
    // Batch them into 75ms buckets so the UI stays butter smooth.
    pub fn start_flushing(self: &Arc<Self>, app: AppHandle) {
        let hub = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(FLUSH_EVERY);
            loop {
                interval.tick().await;
                let batch = std::mem::take(&mut *hub.pending.lock().unwrap_or_else(PoisonError::into_inner));
                if !batch.is_empty() {
                    let _ = app.emit(LOG_EVENT, batch);
                }
            }
        });
    }
}

/// Mirrors launcher `tracing` events (INFO and above) into the hub.
pub struct HubLayer(pub Arc<LogHub>);

impl<S: Subscriber> Layer<S> for HubLayer {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let metadata = event.metadata();
        let level = match *metadata.level() {
            Level::ERROR => LogLevel::Error,
            Level::WARN => LogLevel::Warn,
            Level::INFO => LogLevel::Info,
            _ => return,
        };
        if !metadata.target().starts_with("jifercraft") {
            return;
        }
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        self.0.push(level, LogSource::Launcher, visitor.finish());
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: String,
    fields: String,
}

impl MessageVisitor {
    fn finish(self) -> String {
        if self.fields.is_empty() {
            self.message
        } else {
            format!("{}{}", self.message, self.fields)
        }
    }
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            let _ = write!(self.fields, " {}={value:?}", field.name());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message.push_str(value);
        } else {
            let _ = write!(self.fields, " {}={value}", field.name());
        }
    }
}
