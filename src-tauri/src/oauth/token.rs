use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct TokenGrant {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub expires_in: i64,
}

impl std::fmt::Debug for TokenGrant {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TokenGrant")
            .field("tokens", &"<redacted>")
            .field("expires_in", &self.expires_in)
            .finish()
    }
}
