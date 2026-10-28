// The beating heart of the launcher: settings, game instances, Mojang metadata chaos,
// Java wrangling, and launching Minecraft without melting the user's CPU.

pub mod error;
pub mod fsutil;
pub mod game;
pub mod install;
pub mod instance;
pub mod instances;
pub mod java;
pub mod launch;
pub mod meta;
pub mod paths;
pub mod process;
pub mod settings;
pub mod store;
pub mod timestamp;
pub mod validate;
pub mod version;

#[cfg(test)]
mod testkit;

pub use error::{CoreError, CoreResult};
pub use instance::{Instance, LoaderType};
pub use instances::{InstanceLease, InstanceStore};
pub use paths::{InstanceId, LauncherPaths};
