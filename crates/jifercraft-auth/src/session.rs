use md5::{Digest, Md5};
use serde::Serialize;

use crate::error::{AuthError, AuthResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    Offline,
    Microsoft,
}

/// What the UI shows about the signed-in player. Contains no secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub username: String,
    pub mode: AuthMode,
}

/// What the game needs to start. `access_token` is secret.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub user_type: &'static str,
    pub xuid: Option<String>,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("username", &self.username)
            .field("uuid", &self.uuid)
            .field("user_type", &self.user_type)
            .finish_non_exhaustive()
    }
}

pub fn is_valid_username(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Offline UUID exactly as the Electron build derived it: MD5 of `OfflinePlayer:<name>`, hex-formatted
/// 8-4-4-4-12 without setting version bits. Worlds key player data by this id, so it must not change.
pub fn offline_uuid(name: &str) -> String {
    let digest = hex::encode(Md5::digest(format!("OfflinePlayer:{name}").as_bytes()));
    format!(
        "{}-{}-{}-{}-{}",
        &digest[..8],
        &digest[8..12],
        &digest[12..16],
        &digest[16..20],
        &digest[20..]
    )
}

pub fn offline(name: &str) -> AuthResult<(Session, Credentials)> {
    let name = name.trim();
    if !is_valid_username(name) {
        return Err(AuthError::InvalidUsername);
    }
    let id = offline_uuid(name);
    Ok((
        Session {
            id: id.clone(),
            username: name.to_owned(),
            mode: AuthMode::Offline,
        },
        Credentials {
            username: name.to_owned(),
            uuid: id.clone(),
            access_token: id,
            user_type: "legacy",
            xuid: None,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuids_match_the_electron_build() {
        // Generated with the previous TypeScript implementation.
        assert_eq!(offline_uuid("Steve"), "5627dd98-e6be-bc21-f8a8-e92344183641");
        assert_eq!(offline_uuid("Notch"), "b50ad385-829d-a141-a216-7e7d7539ba7f");
        assert_eq!(offline_uuid("JainaGam3r45"), "29692042-26dd-fa0c-a70d-a4b2309554cc");
        assert_eq!(offline_uuid("abc"), "3d5cec06-bd15-41fa-182f-5dac8c06f1c7");
    }

    #[test]
    fn offline_sessions_validate_and_trim_names() {
        let (session, credentials) = offline("  Steve ").unwrap();
        assert_eq!(session.username, "Steve");
        assert_eq!(session.mode, AuthMode::Offline);
        assert_eq!(credentials.uuid, session.id);
        assert_eq!(credentials.user_type, "legacy");
        for bad in ["ab", "this_name_is_too_long", "bad-name", "espa\u{f1}ol", ""] {
            assert_eq!(offline(bad).unwrap_err().code(), "invalid_username", "{bad}");
        }
    }

    #[test]
    fn credentials_debug_never_prints_the_token() {
        let credentials = Credentials {
            username: "Steve".into(),
            uuid: "uuid".into(),
            access_token: "eyJ-secret".into(),
            user_type: "msa",
            xuid: None,
        };
        assert!(!format!("{credentials:?}").contains("eyJ-secret"));
    }
}
