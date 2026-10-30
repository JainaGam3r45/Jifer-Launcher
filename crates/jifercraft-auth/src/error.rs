#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("use a Minecraft username with 3 to 16 letters, numbers or underscores")]
    InvalidUsername,
    #[error("Microsoft sign-in is not configured in this build")]
    MissingClientId,
    #[error("sign in before launching Minecraft")]
    MissingSession,
    #[error("the Microsoft sign-in code expired; start again")]
    DeviceCodeExpired,
    #[error("the Microsoft sign-in was declined")]
    Declined,
    #[error("the Microsoft session is no longer valid; sign in again")]
    SessionExpired,
    #[error("this Microsoft account has no Xbox profile yet")]
    XboxAccountMissing,
    #[error("child accounts must be added to a family by an adult first")]
    XboxChildAccount,
    #[error("Xbox Live is not available in this account's region")]
    XboxRegionBlocked,
    #[error("this account needs age verification on xbox.com")]
    XboxVerificationRequired,
    #[error("Xbox Live did not return a user hash")]
    XboxMissingUserHash,
    #[error("this Microsoft account does not own Minecraft: Java Edition")]
    ProfileMissing,
    #[error("{step} failed with HTTP {status}")]
    Status { step: &'static str, status: u16 },
    #[error("{step} returned an unexpected response: {reason}")]
    Response { step: &'static str, reason: String },
    #[error("could not reach {step}: {reason}")]
    Network { step: &'static str, reason: String },
    #[error("secure credential storage failed: {0}")]
    SecretStore(String),
    #[error("sign-in cancelled")]
    Cancelled,
}

impl AuthError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidUsername => "invalid_username",
            Self::MissingClientId => "missing_microsoft_client_id",
            Self::MissingSession => "missing_session",
            Self::DeviceCodeExpired => "microsoft_auth_expired",
            Self::Declined => "microsoft_auth_declined",
            Self::SessionExpired => "microsoft_session_expired",
            Self::XboxAccountMissing => "xbox_account_missing",
            Self::XboxChildAccount => "xbox_child_account",
            Self::XboxRegionBlocked => "xbox_region_blocked",
            Self::XboxVerificationRequired => "xbox_verification_required",
            Self::XboxMissingUserHash => "xbox_missing_user_hash",
            Self::ProfileMissing => "minecraft_profile_missing",
            Self::Status { .. } | Self::Response { .. } => "auth_request_failed",
            Self::Network { .. } => "network_unreachable",
            Self::SecretStore(_) => "secret_store_failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Whether a stored refresh token should be kept for a later retry instead of discarded.
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Network { .. })
            || matches!(self, Self::Status { status, .. } if *status == 429 || *status >= 500)
    }
}

pub type AuthResult<T> = Result<T, AuthError>;
