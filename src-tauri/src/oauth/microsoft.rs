use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::oauth::token::TokenGrant;

include!(concat!(env!("OUT_DIR"), "/microsoft_oauth_config.rs"));

pub const MICROSOFT_AUTH_URL: &str =
    "https://login.microsoftonline.com/common/oauth2/v2.0/authorize";
pub const MICROSOFT_TOKEN_URL: &str =
    "https://login.microsoftonline.com/common/oauth2/v2.0/token";
pub const MICROSOFT_PROFILE_URL: &str =
    "https://graph.microsoft.com/v1.0/me?$select=displayName,mail,userPrincipalName,id";
pub const MICROSOFT_SCOPES: &[&str] = &[
    "openid",
    "profile",
    "email",
    "offline_access",
    "https://graph.microsoft.com/IMAP.AccessAsUser.All",
    "https://graph.microsoft.com/SMTP.Send",
    "https://graph.microsoft.com/User.Read",
];
pub const MICROSOFT_OUTLOOK_SCOPES: &[&str] = &[
    "offline_access",
    "https://outlook.office.com/IMAP.AccessAsUser.All",
    "https://outlook.office.com/SMTP.Send",
];

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const NOT_CONFIGURED_REASON: &str = "Microsoft sign-in is not configured in this build.";
const REAUTH_MESSAGE: &str = "Microsoft authorization expired. Reconnect this account.";
const ADMIN_APPROVAL_MESSAGE: &str =
    "A Microsoft administrator must approve AuraMail for this organization before it can be connected.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MicrosoftOAuthStatus {
    pub configured: bool,
    pub reason: Option<String>,
}

#[derive(Clone)]
pub struct MicrosoftClientConfig {
    pub client_id: String,
    pub auth_url: String,
    pub token_url: String,
    pub profile_url: String,
    pub scopes: Vec<String>,
    pub outlook_scopes: Vec<String>,
}

impl std::fmt::Debug for MicrosoftClientConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MicrosoftClientConfig")
            .field("client_id", &self.client_id)
            .field("auth_url", &self.auth_url)
            .field("token_url", &self.token_url)
            .field("profile_url", &self.profile_url)
            .field("scopes", &self.scopes)
            .field("outlook_scopes", &self.outlook_scopes)
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrosoftProfile {
    #[serde(default)]
    pub mail: Option<String>,
    #[serde(default)]
    pub user_principal_name: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
}

impl MicrosoftProfile {
    pub fn email(&self) -> Option<&str> {
        self.mail
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                self.user_principal_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
    }

    pub fn username(&self) -> Option<&str> {
        self.user_principal_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| self.email())
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn resolve_client_id(runtime_id: Option<String>, embedded_id: Option<&str>) -> Option<String> {
    non_empty(runtime_id).or_else(|| non_empty(embedded_id.map(str::to_string)))
}

impl MicrosoftClientConfig {
    pub fn from_env() -> Option<Self> {
        let client_id = resolve_client_id(
            std::env::var("AURAMAIL_MICROSOFT_CLIENT_ID").ok(),
            EMBEDDED_MICROSOFT_CLIENT_ID,
        )?;
        Some(Self {
            client_id,
            auth_url: MICROSOFT_AUTH_URL.into(),
            token_url: MICROSOFT_TOKEN_URL.into(),
            profile_url: MICROSOFT_PROFILE_URL.into(),
            scopes: MICROSOFT_SCOPES.iter().map(|scope| scope.to_string()).collect(),
            outlook_scopes: MICROSOFT_OUTLOOK_SCOPES
                .iter()
                .map(|scope| scope.to_string())
                .collect(),
        })
    }

    pub fn status_from_values(client_id: Option<&str>) -> MicrosoftOAuthStatus {
        let configured = client_id.map_or(false, |value| !value.trim().is_empty());
        MicrosoftOAuthStatus {
            configured,
            reason: (!configured).then(|| NOT_CONFIGURED_REASON.to_string()),
        }
    }
}

pub fn microsoft_status() -> MicrosoftOAuthStatus {
    if MicrosoftClientConfig::from_env().is_some() {
        MicrosoftOAuthStatus {
            configured: true,
            reason: None,
        }
    } else {
        MicrosoftClientConfig::status_from_values(None)
    }
}

pub fn authorization_url(
    config: &MicrosoftClientConfig,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> String {
    let mut url =
        reqwest::Url::parse(&config.auth_url).expect("valid Microsoft authorization URL");
    url.query_pairs_mut()
        .append_pair("client_id", &config.client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("response_mode", "query")
        .append_pair("scope", &config.scopes.join(" "))
        .append_pair("prompt", "select_account")
        .append_pair("state", state)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256");
    url.to_string()
}

pub fn exchange_code(
    config: &MicrosoftClientConfig,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> AppResult<TokenGrant> {
    let client = blocking_client()?;
    let form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("client_id", config.client_id.clone()),
        ("redirect_uri", redirect_uri.to_string()),
        ("code_verifier", code_verifier.to_string()),
        ("scope", config.scopes.join(" ")),
    ];
    let response = client.post(&config.token_url).form(&form).send()?;
    parse_token_response(response)
}

pub fn exchange_for_outlook(
    config: &MicrosoftClientConfig,
    refresh_token: &str,
) -> AppResult<TokenGrant> {
    let client = blocking_client()?;
    let form = vec![
        ("grant_type", "refresh_token".to_string()),
        ("client_id", config.client_id.clone()),
        ("refresh_token", refresh_token.to_string()),
        ("scope", config.outlook_scopes.join(" ")),
    ];
    let response = client.post(&config.token_url).form(&form).send()?;
    parse_token_response(response)
}

pub fn refresh_access_token(
    config: &MicrosoftClientConfig,
    refresh_token: &str,
) -> AppResult<TokenGrant> {
    exchange_for_outlook(config, refresh_token)
}

pub fn fetch_profile(
    config: &MicrosoftClientConfig,
    access_token: &str,
) -> AppResult<MicrosoftProfile> {
    let client = blocking_client()?;
    let response = client
        .get(&config.profile_url)
        .bearer_auth(access_token)
        .send()?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|_| AppError::Message("Microsoft sign-in is unavailable.".into()))?;
    if status.is_success() {
        let profile = serde_json::from_str::<MicrosoftProfile>(&body)?;
        if profile.email().is_none() {
            return Err(AppError::Message(
                "Microsoft did not return an email address.".into(),
            ));
        }
        return Ok(profile);
    }
    Err(parse_microsoft_error(status, &body))
}

pub fn revoke(_config: &MicrosoftClientConfig, _token: &str) -> AppResult<()> {
    // Microsoft does not expose the same simple token-revocation endpoint as
    // Google. The token is removed with the keyring entry instead.
    Ok(())
}

pub fn reauth_message() -> &'static str {
    REAUTH_MESSAGE
}

pub fn admin_approval_message() -> &'static str {
    ADMIN_APPROVAL_MESSAGE
}

pub fn is_admin_consent_error(error: Option<&str>, description: Option<&str>) -> bool {
    let error = error.unwrap_or_default();
    let description = description.unwrap_or_default().to_ascii_lowercase();
    error == "consent_required"
        || description.contains("aadsts65001")
        || description.contains("aadsts90094")
        || description.contains("admin consent")
        || description.contains("administrator approval")
}

pub fn is_user_declined(description: Option<&str>) -> bool {
    description
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains("aadsts65004")
}

fn blocking_client() -> AppResult<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent("AuraMail/0.1")
        .build()
        .map_err(|_| AppError::Message("Microsoft sign-in is unavailable.".into()))
}

fn parse_token_response(response: reqwest::blocking::Response) -> AppResult<TokenGrant> {
    let status = response.status();
    let body = response
        .text()
        .map_err(|_| AppError::Message("Microsoft sign-in is unavailable.".into()))?;
    if status.is_success() {
        return serde_json::from_str::<TokenGrant>(&body).map_err(Into::into);
    }
    Err(parse_microsoft_error(status, &body))
}

fn parse_microsoft_error(status: reqwest::StatusCode, body: &str) -> AppError {
    #[derive(Deserialize)]
    struct MicrosoftErrorBody {
        #[serde(default)]
        error: String,
        #[serde(default)]
        error_description: String,
    }

    let parsed = serde_json::from_str::<MicrosoftErrorBody>(body).unwrap_or(MicrosoftErrorBody {
        error: String::new(),
        error_description: String::new(),
    });

    if parsed.error == "invalid_grant" {
        return AppError::Reauth(REAUTH_MESSAGE.to_string());
    }
    if parsed.error == "invalid_client" {
        return AppError::Message("Microsoft sign-in configuration is invalid.".into());
    }
    if is_admin_consent_error(Some(&parsed.error), Some(&parsed.error_description)) {
        return AppError::Message(ADMIN_APPROVAL_MESSAGE.to_string());
    }
    if status == reqwest::StatusCode::BAD_REQUEST && parsed.error == "invalid_request" {
        return AppError::Message("Microsoft sign-in configuration is invalid.".into());
    }
    AppError::Message("Microsoft sign-in failed. Try again.".into())
}

#[cfg(test)]
mod tests {
    use super::{
        authorization_url, is_admin_consent_error, is_user_declined, MicrosoftClientConfig,
        MicrosoftProfile,
    };

    fn test_config() -> MicrosoftClientConfig {
        MicrosoftClientConfig {
            client_id: "client-id".into(),
            auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
            token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".into(),
            profile_url: "https://graph.microsoft.com/v1.0/me".into(),
            scopes: vec!["openid".into(), "offline_access".into()],
            outlook_scopes: vec!["offline_access".into()],
        }
    }

    #[test]
    fn authorization_url_uses_common_endpoint_and_graph_scopes() {
        let url = authorization_url(
            &test_config(),
            "http://localhost:45678",
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
        assert_eq!(value("redirect_uri"), Some("http://localhost:45678"));
        assert_eq!(value("response_type"), Some("code"));
        assert_eq!(value("response_mode"), Some("query"));
        assert_eq!(value("scope"), Some("openid offline_access"));
        assert_eq!(value("prompt"), Some("select_account"));
        assert_eq!(value("state"), Some("state-123"));
        assert_eq!(value("code_challenge"), Some("challenge-abc"));
        assert_eq!(value("code_challenge_method"), Some("S256"));
    }

    #[test]
    fn profile_prefers_primary_mail_but_keeps_upn_as_username() {
        let profile = MicrosoftProfile {
            mail: Some("nicole@contoso.com".into()),
            user_principal_name: Some("nicole@contoso.onmicrosoft.com".into()),
            display_name: Some("Nicole".into()),
        };
        assert_eq!(profile.email(), Some("nicole@contoso.com"));
        assert_eq!(profile.username(), Some("nicole@contoso.onmicrosoft.com"));
    }

    #[test]
    fn profile_falls_back_to_upn_for_consumer_accounts() {
        let profile = MicrosoftProfile {
            mail: None,
            user_principal_name: Some("me@outlook.com".into()),
            display_name: None,
        };
        assert_eq!(profile.email(), Some("me@outlook.com"));
        assert_eq!(profile.username(), Some("me@outlook.com"));
    }

    #[test]
    fn recognizes_admin_and_user_consent_errors() {
        assert!(is_admin_consent_error(
            Some("access_denied"),
            Some("AADSTS65001: consent is required")
        ));
        assert!(is_admin_consent_error(
            Some("consent_required"),
            Some("The user or administrator has not consented")
        ));
        assert!(is_user_declined(Some("AADSTS65004: User declined")));
    }
}
