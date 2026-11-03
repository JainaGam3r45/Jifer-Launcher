use serde::Serialize;

/// Error shape every command returns. `code` is stable and translated by the UI; `message` is a fallback.
#[derive(Debug, Clone, Serialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

impl IpcError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

pub type IpcResult<T> = Result<T, IpcError>;

macro_rules! from_domain {
    ($($error:ty),+ $(,)?) => {
        $(impl From<$error> for IpcError {
            fn from(error: $error) -> Self {
                Self::new(error.code(), error.to_string())
            }
        })+
    };
}

from_domain!(
    jifercraft_core::CoreError,
    jifercraft_auth::AuthError,
    jifercraft_net::NetError,
    jifercraft_updater::UpdateError,
);

impl From<tauri::Error> for IpcError {
    fn from(error: tauri::Error) -> Self {
        Self::new("shell_error", error.to_string())
    }
}

impl From<tokio::task::JoinError> for IpcError {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::new("task_failed", error.to_string())
    }
}
