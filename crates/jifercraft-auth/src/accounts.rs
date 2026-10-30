//! The signed-in account: restore on startup, sign in/out, and credentials for each launch.

use std::path::Path;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::error::{AuthError, AuthResult};
use crate::microsoft::{AuthHttp, DeviceCode, MicrosoftAccount, MicrosoftAuth};
use crate::session::{self, Credentials, Session};

/// Minecraft access tokens last 24 hours; refresh well before that so a long session never starts stale.
const TOKEN_MAX_AGE: Duration = Duration::from_hours(12);

/// OS-backed storage for the Microsoft refresh token (the only persisted secret).
pub trait SecretStore: Send + Sync + 'static {
    fn load(&self) -> AuthResult<Option<String>>;
    fn save(&self, secret: &str) -> AuthResult<()>;
    fn clear(&self) -> AuthResult<()>;
}

/// Outcome of [`AccountManager::restore`], so the UI can explain a forced sign-out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreOutcome {
    Microsoft,
    Offline,
    SignedOut,
    /// A Microsoft session existed but is no longer valid; the user must sign in again.
    MicrosoftExpired,
    /// The refresh failed for a transient reason; the token is kept for the next attempt.
    MicrosoftUnavailable,
}

enum Current {
    None,
    Offline {
        session: Session,
        credentials: Credentials,
    },
    Microsoft {
        account: MicrosoftAccount,
        refreshed_at: Instant,
    },
}

pub struct AccountManager<H, S> {
    microsoft: Option<MicrosoftAuth<H>>,
    secrets: S,
    current: Mutex<Current>,
}

impl<H, S> std::fmt::Debug for AccountManager<H, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountManager")
            .field("microsoft", &self.microsoft.is_some())
            .finish_non_exhaustive()
    }
}

impl<H: AuthHttp, S: SecretStore> AccountManager<H, S> {
    /// `microsoft` is `None` when the build has no client id; offline play still works.
    pub fn new(microsoft: Option<MicrosoftAuth<H>>, secrets: S) -> Self {
        Self {
            microsoft,
            secrets,
            current: Mutex::new(Current::None),
        }
    }

    pub fn microsoft_available(&self) -> bool {
        self.microsoft.is_some()
    }

    /// Restores the previous Microsoft session, falling back to the remembered offline name.
    pub async fn restore(&self, remembered_offline: Option<&str>) -> RestoreOutcome {
        let stored = self.secrets.load().unwrap_or_else(|error| {
            tracing::warn!(%error, "could not read the stored Microsoft session");
            None
        });
        let mut outcome = RestoreOutcome::SignedOut;
        if let (Some(refresh_token), Some(microsoft)) = (stored, self.microsoft.as_ref()) {
            match microsoft.refresh(&refresh_token).await {
                Ok(account) => {
                    self.store(&account);
                    *self.current.lock().await = Current::Microsoft {
                        account,
                        refreshed_at: Instant::now(),
                    };
                    return RestoreOutcome::Microsoft;
                }
                Err(error) if error.is_transient() => {
                    tracing::warn!(%error, "Microsoft session refresh failed; will retry later");
                    outcome = RestoreOutcome::MicrosoftUnavailable;
                }
                Err(error) => {
                    tracing::info!(%error, "stored Microsoft session is no longer valid");
                    let _ = self.secrets.clear();
                    outcome = RestoreOutcome::MicrosoftExpired;
                }
            }
        }
        if let Some(name) = remembered_offline
            && let Ok((session, credentials)) = session::offline(name)
        {
            *self.current.lock().await = Current::Offline { session, credentials };
            if outcome == RestoreOutcome::SignedOut {
                return RestoreOutcome::Offline;
            }
        }
        outcome
    }

    pub async fn session(&self) -> Option<Session> {
        match &*self.current.lock().await {
            Current::None => None,
            Current::Offline { session, .. } => Some(session.clone()),
            Current::Microsoft { account, .. } => Some(account.session.clone()),
        }
    }

    pub async fn login_offline(&self, name: &str) -> AuthResult<Session> {
        let (session, credentials) = session::offline(name)?;
        *self.current.lock().await = Current::Offline {
            session: session.clone(),
            credentials,
        };
        Ok(session)
    }

    pub async fn start_microsoft(&self) -> AuthResult<DeviceCode> {
        self.microsoft()?.start().await
    }

    pub async fn complete_microsoft(&self, code: &DeviceCode, cancel: &CancellationToken) -> AuthResult<Session> {
        let account = self.microsoft()?.complete(code, cancel).await?;
        self.store(&account);
        let session = account.session.clone();
        *self.current.lock().await = Current::Microsoft {
            account,
            refreshed_at: Instant::now(),
        };
        Ok(session)
    }

    pub async fn logout(&self) -> AuthResult<()> {
        let mut current = self.current.lock().await;
        if matches!(*current, Current::Microsoft { .. }) {
            self.secrets.clear()?;
        }
        *current = Current::None;
        Ok(())
    }

    /// Credentials for a launch, refreshing an ageing Microsoft token first.
    pub async fn credentials(&self) -> AuthResult<Credentials> {
        let mut current = self.current.lock().await;
        match &*current {
            Current::None => Err(AuthError::MissingSession),
            Current::Offline { credentials, .. } => Ok(credentials.clone()),
            Current::Microsoft { account, refreshed_at } if refreshed_at.elapsed() < TOKEN_MAX_AGE => {
                Ok(account.credentials.clone())
            }
            Current::Microsoft { account, .. } => {
                let refreshed = match self.microsoft()?.refresh(&account.refresh_token).await {
                    Ok(refreshed) => refreshed,
                    Err(error) => {
                        if !error.is_transient() {
                            let _ = self.secrets.clear();
                            *current = Current::None;
                        }
                        return Err(error);
                    }
                };
                self.store(&refreshed);
                let credentials = refreshed.credentials.clone();
                *current = Current::Microsoft {
                    account: refreshed,
                    refreshed_at: Instant::now(),
                };
                Ok(credentials)
            }
        }
    }

    fn microsoft(&self) -> AuthResult<&MicrosoftAuth<H>> {
        self.microsoft.as_ref().ok_or(AuthError::MissingClientId)
    }

    fn store(&self, account: &MicrosoftAccount) {
        // Failing to persist only costs a sign-in on next start; the session itself is fine.
        if let Err(error) = self.secrets.save(&account.refresh_token) {
            tracing::warn!(%error, "could not persist the Microsoft session");
        }
    }
}

/// Removes the Electron build's `session.json`. Its payload was encrypted with Electron `safeStorage`,
/// which a non-Electron process cannot decrypt, so the account simply signs in once more.
/// Returns whether a legacy session was found.
pub fn discard_legacy_session(path: &Path) -> bool {
    match std::fs::remove_file(path) {
        Ok(()) => {
            tracing::info!("removed the legacy Electron session; Microsoft accounts need to sign in again");
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            tracing::warn!(%error, "could not remove the legacy Electron session file");
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::microsoft::tests::{ScriptedHttp, endpoints, happy_chain};
    use serde_json::json;
    use std::sync::{Arc, Mutex as StdMutex};

    #[derive(Default, Clone)]
    struct MemorySecrets(Arc<StdMutex<Option<String>>>);

    impl SecretStore for MemorySecrets {
        fn load(&self) -> AuthResult<Option<String>> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn save(&self, secret: &str) -> AuthResult<()> {
            *self.0.lock().unwrap() = Some(secret.to_owned());
            Ok(())
        }
        fn clear(&self) -> AuthResult<()> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    fn manager(http: &ScriptedHttp, secrets: &MemorySecrets) -> AccountManager<ScriptedHttp, MemorySecrets> {
        AccountManager::new(
            Some(MicrosoftAuth::new(http.clone(), "client").with_endpoints(endpoints())),
            secrets.clone(),
        )
    }

    #[tokio::test]
    async fn restores_microsoft_then_falls_back_to_offline() {
        let http = ScriptedHttp::default();
        http.on(
            "token",
            200,
            json!({ "access_token": "ms", "refresh_token": "rotated" }),
        );
        happy_chain(&http);
        let secrets = MemorySecrets::default();
        secrets.save("stored").unwrap();
        let accounts = manager(&http, &secrets);

        assert_eq!(accounts.restore(Some("Alex")).await, RestoreOutcome::Microsoft);
        assert_eq!(accounts.session().await.unwrap().username, "Steve");
        assert_eq!(
            secrets.load().unwrap().as_deref(),
            Some("rotated"),
            "rotated refresh tokens are persisted"
        );

        let offline = manager(&ScriptedHttp::default(), &MemorySecrets::default());
        assert_eq!(offline.restore(Some("Alex")).await, RestoreOutcome::Offline);
        assert_eq!(offline.credentials().await.unwrap().username, "Alex");
        assert_eq!(
            manager(&ScriptedHttp::default(), &MemorySecrets::default())
                .restore(None)
                .await,
            RestoreOutcome::SignedOut
        );
    }

    #[tokio::test]
    async fn revoked_sessions_are_cleared_but_outages_keep_the_token() {
        let revoked = ScriptedHttp::default();
        revoked.on("token", 400, json!({ "error": "invalid_grant" }));
        let secrets = MemorySecrets::default();
        secrets.save("stale").unwrap();
        assert_eq!(
            manager(&revoked, &secrets).restore(None).await,
            RestoreOutcome::MicrosoftExpired
        );
        assert_eq!(secrets.load().unwrap(), None);

        let outage = ScriptedHttp::default();
        outage.on("token", 503, json!({}));
        let secrets = MemorySecrets::default();
        secrets.save("keep-me").unwrap();
        let accounts = manager(&outage, &secrets);
        assert_eq!(
            accounts.restore(Some("Alex")).await,
            RestoreOutcome::MicrosoftUnavailable
        );
        assert_eq!(secrets.load().unwrap().as_deref(), Some("keep-me"));
        assert_eq!(
            accounts.session().await.unwrap().username,
            "Alex",
            "offline play stays available"
        );
    }

    #[tokio::test]
    async fn logout_forgets_the_microsoft_secret() {
        let http = ScriptedHttp::default();
        http.on("token", 200, json!({ "access_token": "ms", "refresh_token": "r" }));
        happy_chain(&http);
        let secrets = MemorySecrets::default();
        secrets.save("r").unwrap();
        let accounts = manager(&http, &secrets);
        accounts.restore(None).await;
        accounts.logout().await.unwrap();
        assert_eq!(accounts.session().await, None);
        assert_eq!(secrets.load().unwrap(), None);
        assert_eq!(accounts.credentials().await.unwrap_err().code(), "missing_session");
    }

    #[tokio::test]
    async fn microsoft_requires_a_client_id() {
        let accounts: AccountManager<ScriptedHttp, MemorySecrets> = AccountManager::new(None, MemorySecrets::default());
        assert!(!accounts.microsoft_available());
        assert_eq!(
            accounts.start_microsoft().await.unwrap_err().code(),
            "missing_microsoft_client_id"
        );
        assert_eq!(accounts.login_offline("Alex").await.unwrap().username, "Alex");
    }

    #[test]
    fn discards_the_legacy_electron_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        std::fs::write(&path, r#"{"mode":"microsoft","payload":"djEwAAAA"}"#).unwrap();
        assert!(discard_legacy_session(&path));
        assert!(!path.exists());
        assert!(!discard_legacy_session(&path));
    }
}
