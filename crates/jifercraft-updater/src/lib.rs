//! Self-update state machine, independent of the host shell.

pub mod notes;
pub mod service;

pub use notes::notes_to_text;
pub use service::{
    CheckOrigin, ChunkFn, GameActivity, Release, Schedule, UpdateDriver, UpdateError, UpdateInfo, UpdateProgress,
    UpdateService, UpdateState, UpdateStatus,
};
