use std::io;
use std::path::PathBuf;

/// Domain errors for the launcher core. `code()` is the stable identifier the UI translates.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("invalid instance id")]
    InvalidInstanceId,
    #[error("path is outside the instances directory")]
    PathOutsideInstances,
    #[error("instance \"{0}\" was not found")]
    InstanceNotFound(String),
    #[error("instance \"{id}\" has an invalid manifest: {reason}")]
    InstanceCorrupt { id: String, reason: String },
    #[error("{0}")]
    InvalidInstance(String),
    #[error("instance \"{0}\" is in use by another operation or a running game")]
    InstanceBusy(String),
    #[error("{0}")]
    InvalidSettings(String),
    #[error("could not move \"{entry}\" into the Default instance: {source}")]
    MigrationFailed {
        entry: String,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Network(#[from] jifercraft_net::NetError),
    #[error("Minecraft version \"{0}\" does not exist")]
    VersionNotFound(String),
    #[error("{what} is invalid: {reason}")]
    Metadata { what: String, reason: String },
    #[error("mod loader \"{0}\" cannot be launched yet")]
    LoaderUnsupported(String),
    #[error("no Java runtime is available: {0}")]
    JavaUnavailable(String),
    #[error("a game is already running or being prepared")]
    AlreadyRunning,
    #[error("the instance has no Minecraft version selected")]
    MissingVersion,
    #[error("Minecraft could not be started: {0}")]
    LaunchFailed(String),
    #[error("operation cancelled")]
    Cancelled,
    #[error("background task failed: {0}")]
    Background(String),
    #[error("could not parse {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("i/o error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl CoreError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInstanceId => "invalid_instance_id",
            Self::PathOutsideInstances => "path_outside_instances",
            Self::InstanceNotFound(_) => "instance_not_found",
            Self::InstanceCorrupt { .. } => "instance_corrupt",
            Self::InvalidInstance(_) => "invalid_instance",
            Self::InstanceBusy(_) => "instance_busy",
            Self::InvalidSettings(_) => "invalid_settings",
            Self::MigrationFailed { .. } => "migration_failed",
            Self::Network(error) => error.code(),
            Self::VersionNotFound(_) => "version_not_found",
            Self::Metadata { .. } => "invalid_metadata",
            Self::LoaderUnsupported(_) => "loader_unsupported",
            Self::JavaUnavailable(_) => "java_unavailable",
            Self::AlreadyRunning => "already_running",
            Self::MissingVersion => "missing_version",
            Self::LaunchFailed(_) => "launch_failed",
            Self::Cancelled => "cancelled",
            Self::Background(_) => "task_failed",
            Self::Json { .. } => "invalid_json",
            Self::Io { .. } => "io_error",
        }
    }

    pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

pub type CoreResult<T> = Result<T, CoreError>;

/// Runs blocking filesystem work off the async executor.
pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> CoreResult<T> + Send + 'static,
) -> CoreResult<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| CoreError::Background(error.to_string()))?
}

/// Attaches the offending path to raw `io::Error`s.
pub(crate) trait IoContext<T> {
    fn at(self, path: impl Into<PathBuf>) -> CoreResult<T>;
}

impl<T> IoContext<T> for io::Result<T> {
    fn at(self, path: impl Into<PathBuf>) -> CoreResult<T> {
        self.map_err(|source| CoreError::io(path, source))
    }
}
