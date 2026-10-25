use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::oauth::token::TokenGrant;

include!(concat!(env!("OUT_DIR"), "/google_oauth_config.rs"));

pub const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const GOOGLE_USERINFO_URL: &str = "https://openidconnect.googleapis.com/v1/userinfo";
pub const GOOGLE_REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
pub const GOOGLE_SCOPES: &str = "openid email profile https://mail.google.com/";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const NOT_CONFIGURED_REASON: &str = "Google sign-in is not configured in this build.";
const REAUTH_MESSAGE: &str = "Google authorization expired. Reconnect this account.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GoogleOAuthStatus {
    pub configured: bool,
    pub reason: Option<String>,
}

#[derive(Clone)]
pub struct GoogleClientConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub auth_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    pub revoke_url: String,
    pub scopes: String,
}

impl std::fmt::Debug for GoogleClientConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GoogleClientConfig")
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<redacted>"),
            )
            .field("auth_url", &self.auth_url)
            .field("token_url", &self.token_url)
            .field("userinfo_url", &self.userinfo_url)
            .field("revoke_url", &self.revoke_url)
            .field("scopes", &self.scopes)
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GoogleProfile {
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email_verified: bool,
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn resolve_credentials(
    runtime_id: Option<String>,
    runtime_secret: Option<String>,
    embedded_id: Option<&str>,
    embedded_secret: Option<&str>,
) -> Option<(String, Option<String>)> {
    if let Some(client_id) = non_empty(runtime_id) {
        return Some((client_id, non_empty(runtime_secret)));
    }
    let client_id = non_empty(embedded_id.map(str::to_string))?;
    Some((
        client_id,
        non_empty(embedded_secret.map(str::to_string)),
    ))
}

impl GoogleClientConfig {
    pub fn from_env() -> Option<Self> {
        let (client_id, client_secret) = resolve_credentials(
            std::env::var("AURAMAIL_GOOGLE_CLIENT_ID").ok(),
            std::env::var("AURAMAIL_GOOGLE_CLIENT_SECRET").ok(),
            EMBEDDED_GOOGLE_CLIENT_ID,
            EMBEDDED_GOOGLE_CLIENT_SECRET,
        )?;

        Some(Self {
            client_id,
            client_secret,
            auth_url: GOOGLE_AUTH_URL.into(),
            token_url: GOOGLE_TOKEN_URL.into(),
            userinfo_url: GOOGLE_USERINFO_URL.into(),
            revoke_url: GOOGLE_REVOKE_URL.into(),
            scopes: GOOGLE_SCOPES.into(),
        })
    }

    pub fn status_from_values(
        client_id: Option<&str>,
        _client_secret: Option<&str>,
    ) -> GoogleOAuthStatus {
        let configured = client_id.map_or(false, |value| !value.trim().is_empty());
        GoogleOAuthStatus {
            configured,
            reason: (!configured).then(|| NOT_CONFIGURED_REASON.to_string()),
        }
    }
}

pub fn google_status() -> GoogleOAuthStatus {
    if GoogleClientConfig::from_env().is_some() {
        GoogleOAuthStatus {
            configured: true,
            reason: None,
        }
    } else {
        GoogleClientConfig::status_from_values(None, None)
    }
}

pub fn authorization_url(
    config: &GoogleClientConfig,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> String {
    let mut url = reqwest::Url::parse(&config.auth_url).expect("valid Google authorization URL");
    url.query_pairs_mut()
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &config.scopes)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "select_account consent")
        .append_pair("state", state)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256");
    url.to_string()
}

pub fn exchange_code(
    config: &GoogleClientConfig,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> AppResult<TokenGrant> {
    let client = blocking_client()?;
    let mut form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("client_id", config.client_id.clone()),
        ("redirect_uri", redirect_uri.to_string()),
        ("code_verifier", code_verifier.to_string()),
    ];
    if let Some(client_secret) = &config.client_secret {
        form.push(("client_secret", client_secret.clone()));
    }
    let response = client.post(&config.token_url).form(&form).send()?;
    parse_token_response(response)
}

pub fn refresh_access_token(
    config: &GoogleClientConfig,
    refresh_token: &str,
) -> AppResult<TokenGrant> {
    let client = blocking_client()?;
    let mut form = vec![
        ("grant_type", "refresh_token".to_string()),
        ("client_id", config.client_id.clone()),
        ("refresh_token", refresh_token.to_string()),
    ];
    if let Some(client_secret) = &config.client_secret {
        form.push(("client_secret", client_secret.clone()));
    }
    let response = client.post(&config.token_url).form(&form).send()?;
    parse_token_response(response)
}

pub fn fetch_profile(config: &GoogleClientConfig, access_token: &str) -> AppResult<GoogleProfile> {
    let client = blocking_client()?;
    let response = client
        .get(&config.userinfo_url)
        .bearer_auth(access_token)
        .send()?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|_| AppError::Message("Google sign-in is unavailable.".into()))?;
    if status.is_success() {
        return serde_json::from_str::<GoogleProfile>(&body).map_err(Into::into);
    }
    Err(parse_google_error(status, &body))
}

pub fn revoke(config: &GoogleClientConfig, token: &str) -> AppResult<()> {
    let client = blocking_client()?;
    let response = client
        .post(&config.revoke_url)
        .form(&[("token", token)])
        .send()?;
    // Revocation is best-effort; any server response is treated as success.
    let _ = response;
    Ok(())
}

fn blocking_client() -> AppResult<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent("AuraMail/0.1")
        .build()
        .map_err(|_| AppError::Message("Google sign-in is unavailable.".into()))
}

fn parse_token_response(response: reqwest::blocking::Response) -> AppResult<TokenGrant> {
    let status = response.status();
    let body = response
        .text()
        .map_err(|_| AppError::Message("Google sign-in is unavailable.".into()))?;
    if status.is_success() {
        return serde_json::from_str::<TokenGrant>(&body).map_err(Into::into);
    }
    Err(parse_google_error(status, &body))
}

fn parse_google_error(status: reqwest::StatusCode, body: &str) -> AppError {
    #[derive(Deserialize)]
    struct GoogleErrorBody {
        #[serde(default)]
        error: String,
    }

    let error_code = serde_json::from_str::<GoogleErrorBody>(body)
        .map(|parsed| parsed.error)
        .unwrap_or_default();

    if error_code == "invalid_grant" {
        return AppError::Reauth(REAUTH_MESSAGE.to_string());
    }
    if status == reqwest::StatusCode::BAD_REQUEST && error_code == "invalid_client" {
        return AppError::Message("Google sign-in configuration is invalid.".into());
    }
    AppError::Message("Google sign-in failed. Try again.".into())
}

#[cfg(test)]
mod tests {
    use super::{authorization_url, resolve_credentials, GoogleClientConfig};
    use crate::oauth::token::TokenGrant;

    #[test]
    fn resolves_runtime_then_embedded_credentials() {
        let runtime = resolve_credentials(
            Some("env-id".into()),
            Some("env-secret".into()),
            Some("file-id"),
            Some("file-secret"),
        )
        .unwrap();
        assert_eq!(runtime.0, "env-id");
        assert_eq!(runtime.1.as_deref(), Some("env-secret"));

        let embedded =
            resolve_credentials(None, None, Some("file-id"), Some("file-secret")).unwrap();
        assert_eq!(embedded.0, "file-id");
        assert_eq!(embedded.1.as_deref(), Some("file-secret"));

        let id_only = resolve_credentials(Some("env-id".into()), None, None, None).unwrap();
        assert_eq!(id_only.0, "env-id");
        assert_eq!(id_only.1, None);
        assert!(GoogleClientConfig::status_from_values(Some("env-id"), None).configured);

        let id_only_runtime = resolve_credentials(
            Some("env-id".into()),
            Some("".into()),
            Some("file-id"),
            Some("file-secret"),
        )
        .unwrap();
        assert_eq!(id_only_runtime.0, "env-id");
        assert_eq!(id_only_runtime.1, None);
    }

    #[test]
    fn missing_google_config_disables_sign_in() {
        let status = GoogleClientConfig::status_from_values(None, None);
        assert!(!status.configured);
        assert_eq!(
            status.reason.as_deref(),
            Some("Google sign-in is not configured in this build.")
        );
    }

    #[test]
    fn authorization_url_sets_offline_pkce_and_scope() {
        let config = GoogleClientConfig {
            client_id: "client-id".into(),
            client_secret: Some("client-secret".into()),
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            userinfo_url: "https://openidconnect.googleapis.com/v1/userinfo".into(),
            revoke_url: "https://oauth2.googleapis.com/revoke".into(),
            scopes: "openid email profile https://mail.google.com/".into(),
        };
        let url = authorization_url(
            &config,
            "http://127.0.0.1:45678",
            "state-123",
            "challenge-abc",
        );
        let parsed = reqwest::Url::parse(&url).unwrap();
        let params: Vec<(String, String)> = parsed
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();
        let value = |key: &str| {
            params
                .iter()
                .find(|(candidate, _)| candidate == key)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(value("client_id"), Some("client-id"));
        assert_eq!(value("redirect_uri"), Some("http://127.0.0.1:45678"));
        assert_eq!(value("response_type"), Some("code"));
        assert_eq!(
            value("scope"),
            Some("openid email profile https://mail.google.com/")
        );
        assert_eq!(value("access_type"), Some("offline"));
        assert_eq!(value("prompt"), Some("select_account consent"));
        assert_eq!(value("state"), Some("state-123"));
        assert_eq!(value("code_challenge"), Some("challenge-abc"));
        assert_eq!(value("code_challenge_method"), Some("S256"));
    }

    #[test]
    fn config_debug_redacts_the_client_secret() {
        let config = GoogleClientConfig {
            client_id: "client-id".into(),
            client_secret: Some("super-secret".into()),
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            userinfo_url: "https://openidconnect.googleapis.com/v1/userinfo".into(),
            revoke_url: "https://oauth2.googleapis.com/revoke".into(),
            scopes: "openid email profile https://mail.google.com/".into(),
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("client-id"));
        assert!(!debug.contains("super-secret"));
    }

    #[test]
    fn token_grant_debug_redacts_tokens() {
        let grant = TokenGrant {
            access_token: "access-token".into(),
            refresh_token: Some("refresh-token".into()),
            expires_in: 3600,
        };
        let debug = format!("{grant:?}");
        assert!(!debug.contains("access-token"));
        assert!(!debug.contains("refresh-token"));
    }
}
