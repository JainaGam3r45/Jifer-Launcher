use jifercraft_auth::{AuthError, AuthResult, SecretStore};

const SERVICE: &str = "com.jifercraft.launcher";
const ACCOUNT: &str = "microsoft-refresh-token";

// Windows Credential Manager has a hard 2560-byte blob limit (CRED_MAX_CREDENTIAL_BLOB_SIZE).
// The standard keyring `set_password` turns strings into UTF-16 (2 bytes per character),
// which detonates with ERROR_INVALID_PARAMETER (87) on beefy Microsoft refresh tokens (~1300+ chars).
// We store raw UTF-8 bytes via `set_secret` to stay safely under the radar.
#[derive(Debug, Default)]
pub struct KeyringStore;

impl KeyringStore {
    fn entry() -> AuthResult<keyring::Entry> {
        keyring::Entry::new(SERVICE, ACCOUNT).map_err(|error| AuthError::SecretStore(error.to_string()))
    }
}

impl SecretStore for KeyringStore {
    fn load(&self) -> AuthResult<Option<String>> {
        match Self::entry()?.get_secret() {
            Ok(secret) => String::from_utf8(secret)
                .map(Some)
                .map_err(|_| AuthError::SecretStore("stored session is not valid UTF-8".to_owned())),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(AuthError::SecretStore(error.to_string())),
        }
    }

    fn save(&self, secret: &str) -> AuthResult<()> {
        Self::entry()?
            .set_secret(secret.as_bytes())
            .map_err(|error| AuthError::SecretStore(error.to_string()))
    }

    fn clear(&self) -> AuthResult<()> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(AuthError::SecretStore(error.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    /// Touches the real OS vault, so it only runs on request: `cargo test -p jifercraft-desktop -- --ignored`.
    #[test]
    #[ignore = "uses the OS credential vault"]
    fn vault_round_trips_a_long_refresh_token() {
        let entry = keyring::Entry::new("com.jifercraft.launcher.selftest", "long-token").unwrap();
        let token = "M.C5".to_owned() + &"x".repeat(2000);
        let utf16 = entry.set_password(&token);
        let bytes = entry.set_secret(token.as_bytes());
        let read = entry.get_secret().map(|secret| secret == token.as_bytes());
        let _ = entry.delete_credential();
        eprintln!("set_password: {utf16:?} | set_secret: {bytes:?} | read back: {read:?}");
        assert!(bytes.is_ok() && matches!(read, Ok(true)));
    }
}
