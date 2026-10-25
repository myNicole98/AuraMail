use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::oauth::token::TokenGrant;

const VERSION: u32 = 1;
const EXPIRY_MARGIN_SECS: i64 = 60;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthSecret {
    #[serde(default = "default_version")]
    pub version: u32,
    pub provider: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub expires_at: i64,
}

fn default_version() -> u32 {
    VERSION
}

impl OAuthSecret {
    pub fn new(provider: &str, grant: &TokenGrant, now: i64) -> AppResult<Self> {
        let refresh_token = grant.refresh_token.clone().ok_or_else(|| {
            AppError::Message(
                "The provider did not return a reusable authorization. Try again.".into(),
            )
        })?;
        Ok(Self {
            version: VERSION,
            provider: provider.to_string(),
            refresh_token,
            access_token: grant.access_token.clone(),
            expires_at: expiry_from(now, grant.expires_in),
        })
    }

    pub fn apply_grant(&mut self, grant: &TokenGrant, now: i64) {
        self.access_token = grant.access_token.clone();
        self.expires_at = expiry_from(now, grant.expires_in);
        if let Some(refresh_token) = &grant.refresh_token {
            self.refresh_token = refresh_token.clone();
        }
    }

    pub fn is_fresh(&self, now: i64) -> bool {
        now < self.expires_at
    }

    pub fn encode(&self) -> AppResult<String> {
        serde_json::to_string(self).map_err(Into::into)
    }

    pub fn parse(value: &str) -> AppResult<Self> {
        serde_json::from_str(value).map_err(Into::into)
    }
}

fn expiry_from(now: i64, expires_in: i64) -> i64 {
    now + expires_in - EXPIRY_MARGIN_SECS
}

impl std::fmt::Debug for OAuthSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OAuthSecret")
            .field("version", &self.version)
            .field("provider", &self.provider)
            .field("expires_at", &self.expires_at)
            .field("tokens", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::OAuthSecret;
    use crate::oauth::token::TokenGrant;

    #[test]
    fn secret_expires_early_and_round_trips() {
        let grant = TokenGrant {
            access_token: "access".into(),
            refresh_token: Some("refresh".into()),
            expires_in: 3600,
        };
        let secret = OAuthSecret::new("google", &grant, 1_000).unwrap();
        assert_eq!(secret.expires_at, 4_540);
        assert!(secret.is_fresh(1_100));
        assert!(!secret.is_fresh(4_540));
        assert_eq!(OAuthSecret::parse(&secret.encode().unwrap()).unwrap(), secret);
        assert!(!format!("{secret:?}").contains("access"));
        assert!(!format!("{secret:?}").contains("refresh"));
    }
}
