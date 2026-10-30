//! Offline and Microsoft account sessions.

pub mod accounts;
pub mod error;
pub mod microsoft;
pub mod session;

pub use accounts::{AccountManager, RestoreOutcome, SecretStore, discard_legacy_session};
pub use error::{AuthError, AuthResult};
pub use microsoft::{AuthHttp, DeviceCode, Endpoints, MicrosoftAccount, MicrosoftAuth};
pub use session::{AuthMode, Credentials, Session, offline_uuid};
