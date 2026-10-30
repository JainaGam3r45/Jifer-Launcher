// Microsoft OAuth: 5 API roundtrips just to verify you own a block game.
// Device code -> MSA token -> Xbox Live user token -> XSTS token -> Minecraft token -> profile.
// If any link in this bureaucratic chain sneezes, the whole login collapses.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::error::{AuthError, AuthResult};
use crate::session::{AuthMode, Credentials, Session};

const SCOPE: &str = "XboxLive.signin offline_access";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

#[derive(Debug, Clone)]
pub struct Endpoints {
    pub device_code: String,
    pub token: String,
    pub xbox_user: String,
    pub xsts: String,
    pub minecraft_login: String,
    pub minecraft_profile: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            device_code: "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode".into(),
            token: "https://login.microsoftonline.com/consumers/oauth2/v2.0/token".into(),
            xbox_user: "https://user.auth.xboxlive.com/user/authenticate".into(),
            xsts: "https://xsts.auth.xboxlive.com/xsts/authorize".into(),
            minecraft_login: "https://api.minecraftservices.com/authentication/login_with_xbox".into(),
            minecraft_profile: "https://api.minecraftservices.com/minecraft/profile".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpReply {
    pub status: u16,
    pub body: Vec<u8>,
}

/// The three request shapes the sign-in flow needs; a seam so tests can script Microsoft's answers.
pub trait AuthHttp: Send + Sync + 'static {
    fn post_form(&self, url: &str, form: &[(&str, &str)]) -> impl Future<Output = Result<HttpReply, String>> + Send;
    fn post_json(&self, url: &str, body: &Value) -> impl Future<Output = Result<HttpReply, String>> + Send;
    fn get_bearer(&self, url: &str, token: &str) -> impl Future<Output = Result<HttpReply, String>> + Send;
}

impl AuthHttp for jifercraft_net::HttpClient {
    async fn post_form(&self, url: &str, form: &[(&str, &str)]) -> Result<HttpReply, String> {
        let response = self
            .inner()
            .post(url)
            .form(form)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        reply(response).await
    }

    async fn post_json(&self, url: &str, body: &Value) -> Result<HttpReply, String> {
        let response = self
            .inner()
            .post(url)
            .header("accept", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        reply(response).await
    }

    async fn get_bearer(&self, url: &str, token: &str) -> Result<HttpReply, String> {
        let response = self
            .inner()
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        reply(response).await
    }
}

async fn reply(response: reqwest::Response) -> Result<HttpReply, String> {
    let status = response.status().as_u16();
    let body = response.bytes().await.map_err(|error| error.to_string())?.to_vec();
    Ok(HttpReply { status, body })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    pub message: String,
}

/// A signed-in Microsoft player. `refresh_token` is the only part worth persisting.
#[derive(Clone)]
pub struct MicrosoftAccount {
    pub session: Session,
    pub credentials: Credentials,
    pub refresh_token: String,
}

impl std::fmt::Debug for MicrosoftAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MicrosoftAccount")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct MicrosoftAuth<H> {
    http: H,
    client_id: String,
    endpoints: Endpoints,
}

#[derive(Deserialize)]
struct DeviceCodeReply {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: Option<u64>,
    message: String,
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct OAuthError {
    error: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XboxReply {
    token: String,
    display_claims: DisplayClaims,
}

#[derive(Deserialize)]
struct DisplayClaims {
    xui: Vec<XboxUser>,
}

#[derive(Deserialize)]
struct XboxUser {
    uhs: String,
    #[serde(default)]
    xid: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XstsError {
    x_err: u64,
}

#[derive(Deserialize)]
struct MinecraftLogin {
    access_token: String,
}

#[derive(Deserialize)]
struct Profile {
    id: String,
    name: String,
}

impl<H: AuthHttp> MicrosoftAuth<H> {
    pub fn new(http: H, client_id: impl Into<String>) -> Self {
        Self {
            http,
            client_id: client_id.into(),
            endpoints: Endpoints::default(),
        }
    }

    #[must_use]
    pub fn with_endpoints(mut self, endpoints: Endpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    pub async fn start(&self) -> AuthResult<DeviceCode> {
        let reply = self
            .send_form(
                "device code request",
                &self.endpoints.device_code,
                &[("client_id", &self.client_id), ("scope", SCOPE)],
            )
            .await?;
        let code: DeviceCodeReply = ok_json("device code request", &reply)?;
        Ok(DeviceCode {
            device_code: code.device_code,
            user_code: code.user_code,
            verification_uri: code.verification_uri,
            expires_in: code.expires_in,
            interval: code.interval.unwrap_or(5),
            message: code.message,
        })
    }

    /// Polls until the user approves the code, honouring the server's `interval` and `slow_down`.
    pub async fn complete(&self, code: &DeviceCode, cancel: &CancellationToken) -> AuthResult<MicrosoftAccount> {
        let mut interval = Duration::from_secs(code.interval.max(1));
        let deadline = tokio::time::Instant::now() + Duration::from_secs(code.expires_in.clamp(1, 15 * 60));
        loop {
            tokio::select! {
                () = tokio::time::sleep(interval) => {}
                () = cancel.cancelled() => return Err(AuthError::Cancelled),
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(AuthError::DeviceCodeExpired);
            }
            let form = [
                ("grant_type", DEVICE_GRANT),
                ("client_id", self.client_id.as_str()),
                ("device_code", code.device_code.as_str()),
            ];
            let reply = self
                .send_form("Microsoft token request", &self.endpoints.token, &form)
                .await?;
            if reply.status == 200 {
                let token: TokenReply = ok_json("Microsoft token request", &reply)?;
                return self.account_from(token).await;
            }
            let error = serde_json::from_slice::<OAuthError>(&reply.body)
                .map(|e| e.error)
                .unwrap_or_default();
            match error.as_str() {
                "authorization_pending" => {}
                "slow_down" => interval += Duration::from_secs(5),
                "authorization_declined" | "access_denied" => return Err(AuthError::Declined),
                "expired_token" | "bad_verification_code" => return Err(AuthError::DeviceCodeExpired),
                _ => {
                    return Err(AuthError::Status {
                        step: "Microsoft token request",
                        status: reply.status,
                    });
                }
            }
        }
    }

    /// Exchanges a stored refresh token for a fresh Minecraft session.
    pub async fn refresh(&self, refresh_token: &str) -> AuthResult<MicrosoftAccount> {
        let form = [
            ("grant_type", "refresh_token"),
            ("client_id", self.client_id.as_str()),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ];
        let reply = self
            .send_form("Microsoft token refresh", &self.endpoints.token, &form)
            .await?;
        if reply.status == 400 || reply.status == 401 {
            return Err(AuthError::SessionExpired);
        }
        let mut token: TokenReply = ok_json("Microsoft token refresh", &reply)?;
        if token.refresh_token.is_none() {
            token.refresh_token = Some(refresh_token.to_owned());
        }
        self.account_from(token).await
    }

    async fn account_from(&self, token: TokenReply) -> AuthResult<MicrosoftAccount> {
        let refresh_token = token.refresh_token.ok_or(AuthError::Response {
            step: "Microsoft token request",
            reason: "no refresh token (is offline_access granted?)".into(),
        })?;

        let xbox_body = json!({
            "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={}", token.access_token) },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        });
        let xbox: XboxReply = ok_json(
            "Xbox Live sign-in",
            &self
                .send_json("Xbox Live sign-in", &self.endpoints.xbox_user, &xbox_body)
                .await?,
        )?;

        let xsts_body = json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbox.token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        });
        let xsts_reply = self
            .send_json("Xbox XSTS authorization", &self.endpoints.xsts, &xsts_body)
            .await?;
        if xsts_reply.status == 401 {
            return Err(xsts_failure(&xsts_reply.body));
        }
        let xsts: XboxReply = ok_json("Xbox XSTS authorization", &xsts_reply)?;
        let user = xsts
            .display_claims
            .xui
            .into_iter()
            .next()
            .ok_or(AuthError::XboxMissingUserHash)?;

        let login_body = json!({ "identityToken": format!("XBL3.0 x={};{}", user.uhs, xsts.token) });
        let login: MinecraftLogin = ok_json(
            "Minecraft login",
            &self
                .send_json("Minecraft login", &self.endpoints.minecraft_login, &login_body)
                .await?,
        )?;

        let profile_reply = self
            .http
            .get_bearer(&self.endpoints.minecraft_profile, &login.access_token)
            .await
            .map_err(|reason| AuthError::Network {
                step: "Minecraft profile",
                reason,
            })?;
        if profile_reply.status == 404 {
            return Err(AuthError::ProfileMissing);
        }
        let profile: Profile = ok_json("Minecraft profile", &profile_reply)?;

        Ok(MicrosoftAccount {
            session: Session {
                id: profile.id.clone(),
                username: profile.name.clone(),
                mode: AuthMode::Microsoft,
            },
            credentials: Credentials {
                username: profile.name,
                uuid: profile.id,
                access_token: login.access_token,
                user_type: "msa",
                xuid: user.xid,
            },
            refresh_token,
        })
    }

    async fn send_form(&self, step: &'static str, url: &str, form: &[(&str, &str)]) -> AuthResult<HttpReply> {
        self.http
            .post_form(url, form)
            .await
            .map_err(|reason| AuthError::Network { step, reason })
    }

    async fn send_json(&self, step: &'static str, url: &str, body: &Value) -> AuthResult<HttpReply> {
        self.http
            .post_json(url, body)
            .await
            .map_err(|reason| AuthError::Network { step, reason })
    }
}

fn ok_json<T: serde::de::DeserializeOwned>(step: &'static str, reply: &HttpReply) -> AuthResult<T> {
    if !(200..300).contains(&reply.status) {
        return Err(AuthError::Status {
            step,
            status: reply.status,
        });
    }
    serde_json::from_slice(&reply.body).map_err(|error| AuthError::Response {
        step,
        reason: error.to_string(),
    })
}

// Mojang / Xbox error codes: undocumented magic numbers excavated from bug trackers and forums.
fn xsts_failure(body: &[u8]) -> AuthError {
    match serde_json::from_slice::<XstsError>(body).map(|error| error.x_err) {
        // 2148916233: User doesn't have an Xbox profile. Yes, in 2026 people still need to visit xbox.com first.
        Ok(2_148_916_233) => AuthError::XboxAccountMissing,
        // 2148916238: Underage/child account; needs a parent to click "allow" in Microsoft Family Safety.
        Ok(2_148_916_238) => AuthError::XboxChildAccount,
        // 2148916235: Xbox Live isn't supported in the user's country/region.
        Ok(2_148_916_235) => AuthError::XboxRegionBlocked,
        // 2148916236 / 2148916237: Adult verification required.
        Ok(2_148_916_236 | 2_148_916_237) => AuthError::XboxVerificationRequired,
        _ => AuthError::Status {
            step: "Xbox XSTS authorization",
            status: 401,
        },
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::{HashMap, VecDeque};
    use std::sync::{Arc, Mutex};

    /// Scripted replies per URL, consumed in order; the last one repeats.
    #[derive(Default, Clone)]
    pub struct ScriptedHttp {
        replies: Arc<Mutex<HashMap<String, VecDeque<HttpReply>>>>,
        pub calls: Arc<Mutex<Vec<String>>>,
    }

    impl ScriptedHttp {
        #[expect(
            clippy::needless_pass_by_value,
            reason = "call sites read better with json! literals"
        )]
        pub fn on(&self, url: &str, status: u16, body: Value) -> &Self {
            self.replies
                .lock()
                .unwrap()
                .entry(url.to_owned())
                .or_default()
                .push_back(HttpReply {
                    status,
                    body: body.to_string().into_bytes(),
                });
            self
        }

        fn next(&self, url: &str) -> Result<HttpReply, String> {
            self.calls.lock().unwrap().push(url.to_owned());
            let mut replies = self.replies.lock().unwrap();
            let queue = replies
                .get_mut(url)
                .ok_or_else(|| format!("unexpected request to {url}"))?;
            if queue.len() > 1 {
                queue.pop_front()
            } else {
                queue.front().cloned()
            }
            .ok_or_else(|| "empty".into())
        }
    }

    impl AuthHttp for ScriptedHttp {
        fn post_form(&self, url: &str, _: &[(&str, &str)]) -> impl Future<Output = Result<HttpReply, String>> + Send {
            std::future::ready(self.next(url))
        }
        fn post_json(&self, url: &str, _: &Value) -> impl Future<Output = Result<HttpReply, String>> + Send {
            std::future::ready(self.next(url))
        }
        fn get_bearer(&self, url: &str, _: &str) -> impl Future<Output = Result<HttpReply, String>> + Send {
            std::future::ready(self.next(url))
        }
    }

    pub fn endpoints() -> Endpoints {
        Endpoints {
            device_code: "device".into(),
            token: "token".into(),
            xbox_user: "xbox".into(),
            xsts: "xsts".into(),
            minecraft_login: "login".into(),
            minecraft_profile: "profile".into(),
        }
    }

    pub fn happy_chain(http: &ScriptedHttp) {
        http.on(
            "xbox",
            200,
            json!({ "Token": "xbl", "DisplayClaims": { "xui": [{ "uhs": "hash" }] } }),
        )
        .on(
            "xsts",
            200,
            json!({ "Token": "xsts", "DisplayClaims": { "xui": [{ "uhs": "hash", "xid": "2535" }] } }),
        )
        .on("login", 200, json!({ "access_token": "mc-token", "expires_in": 86400 }))
        .on(
            "profile",
            200,
            json!({ "id": "4566e69fc90748ee8d71d7ba5aa00d20", "name": "Steve" }),
        );
    }

    fn auth(http: &ScriptedHttp) -> MicrosoftAuth<ScriptedHttp> {
        MicrosoftAuth::new(http.clone(), "client").with_endpoints(endpoints())
    }

    fn code(interval: u64) -> DeviceCode {
        DeviceCode {
            device_code: "dc".into(),
            user_code: "ABCD".into(),
            verification_uri: "https://microsoft.com/link".into(),
            expires_in: 900,
            interval,
            message: String::new(),
        }
    }

    #[tokio::test]
    async fn starts_the_device_flow() {
        let http = ScriptedHttp::default();
        http.on("device", 200, json!({ "device_code": "dc", "user_code": "ABCD", "verification_uri": "https://microsoft.com/link", "expires_in": 900, "message": "go" }));
        let code = auth(&http).start().await.unwrap();
        assert_eq!((code.user_code.as_str(), code.interval), ("ABCD", 5));
    }

    #[tokio::test(start_paused = true)]
    async fn polls_with_backoff_then_builds_the_session() {
        let http = ScriptedHttp::default();
        http.on("token", 400, json!({ "error": "authorization_pending" }))
            .on("token", 400, json!({ "error": "slow_down" }))
            .on(
                "token",
                200,
                json!({ "access_token": "ms", "refresh_token": "refresh" }),
            );
        happy_chain(&http);
        let started = tokio::time::Instant::now();

        let account = auth(&http).complete(&code(2), &CancellationToken::new()).await.unwrap();

        assert_eq!(
            started.elapsed(),
            Duration::from_secs(2 + 2 + 7),
            "slow_down adds five seconds"
        );
        assert_eq!(account.session.username, "Steve");
        assert_eq!(account.session.mode, AuthMode::Microsoft);
        assert_eq!(account.credentials.access_token, "mc-token");
        assert_eq!(account.credentials.xuid.as_deref(), Some("2535"));
        assert_eq!(account.refresh_token, "refresh");
        assert!(!format!("{account:?}").contains("refresh"));
    }

    #[tokio::test(start_paused = true)]
    async fn declined_expired_and_cancelled_flows() {
        let http = ScriptedHttp::default();
        http.on("token", 400, json!({ "error": "authorization_declined" }));
        assert_eq!(
            auth(&http)
                .complete(&code(1), &CancellationToken::new())
                .await
                .unwrap_err()
                .code(),
            "microsoft_auth_declined"
        );

        let http = ScriptedHttp::default();
        http.on("token", 400, json!({ "error": "expired_token" }));
        assert_eq!(
            auth(&http)
                .complete(&code(1), &CancellationToken::new())
                .await
                .unwrap_err()
                .code(),
            "microsoft_auth_expired"
        );

        let cancel = CancellationToken::new();
        cancel.cancel();
        assert_eq!(
            auth(&ScriptedHttp::default())
                .complete(&code(1), &cancel)
                .await
                .unwrap_err()
                .code(),
            "cancelled"
        );
    }

    #[tokio::test]
    async fn maps_xsts_and_profile_failures() {
        for (xerr, code) in [
            (2_148_916_233_u64, "xbox_account_missing"),
            (2_148_916_238, "xbox_child_account"),
        ] {
            let http = ScriptedHttp::default();
            http.on("token", 200, json!({ "access_token": "ms", "refresh_token": "r" }))
                .on(
                    "xbox",
                    200,
                    json!({ "Token": "xbl", "DisplayClaims": { "xui": [{ "uhs": "h" }] } }),
                )
                .on("xsts", 401, json!({ "XErr": xerr }));
            assert_eq!(auth(&http).refresh("r").await.unwrap_err().code(), code);
        }

        let http = ScriptedHttp::default();
        happy_chain(&http);
        http.on("token", 200, json!({ "access_token": "ms" }));
        let replies = http.replies.clone();
        replies.lock().unwrap().insert(
            "profile".into(),
            VecDeque::from([HttpReply {
                status: 404,
                body: Vec::new(),
            }]),
        );
        assert_eq!(
            auth(&http).refresh("r").await.unwrap_err().code(),
            "minecraft_profile_missing"
        );
    }

    #[tokio::test]
    async fn refresh_keeps_the_old_token_and_flags_revocation() {
        let http = ScriptedHttp::default();
        http.on("token", 200, json!({ "access_token": "ms" }));
        happy_chain(&http);
        assert_eq!(
            auth(&http).refresh("old-refresh").await.unwrap().refresh_token,
            "old-refresh"
        );

        let revoked = ScriptedHttp::default();
        revoked.on("token", 400, json!({ "error": "invalid_grant" }));
        let error = auth(&revoked).refresh("old").await.unwrap_err();
        assert_eq!(error.code(), "microsoft_session_expired");
        assert!(!error.is_transient());
    }
}
