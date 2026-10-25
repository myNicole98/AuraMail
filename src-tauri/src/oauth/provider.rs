use crate::error::{AppError, AppResult};
use crate::oauth::google;
use crate::oauth::microsoft;
use crate::oauth::token::TokenGrant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthProvider {
    Google,
    Microsoft,
}

impl OAuthProvider {
    pub fn id(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Microsoft => "microsoft",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Google => "Google",
            Self::Microsoft => "Microsoft",
        }
    }

    pub fn from_id(value: &str) -> Option<Self> {
        match value {
            "microsoft" => Some(Self::Microsoft),
            // Empty provider is the legacy representation for Google OAuth
            // accounts created before the provider field was populated.
            "" | "google" => Some(Self::Google),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OAuthProfile {
    pub email: String,
    pub username: String,
    pub name: Option<String>,
    pub email_verified: bool,
}

pub struct OAuthGrant {
    pub provider: OAuthProvider,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub profile: OAuthProfile,
}

impl std::fmt::Debug for OAuthGrant {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OAuthGrant")
            .field("provider", &self.provider)
            .field("tokens", &"<redacted>")
            .field("expires_in", &self.expires_in)
            .field("profile", &self.profile)
            .finish()
    }
}

#[derive(Clone)]
pub enum OAuthClientConfig {
    Google(google::GoogleClientConfig),
    Microsoft(microsoft::MicrosoftClientConfig),
}

impl std::fmt::Debug for OAuthClientConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Google(config) => formatter.debug_tuple("Google").field(config).finish(),
            Self::Microsoft(config) => formatter.debug_tuple("Microsoft").field(config).finish(),
        }
    }
}

impl OAuthClientConfig {
    pub fn from_env(provider: OAuthProvider) -> Option<Self> {
        match provider {
            OAuthProvider::Google => google::GoogleClientConfig::from_env().map(Self::Google),
            OAuthProvider::Microsoft => {
                microsoft::MicrosoftClientConfig::from_env().map(Self::Microsoft)
            }
        }
    }

    pub fn for_provider(provider: OAuthProvider) -> AppResult<Self> {
        Self::from_env(provider).ok_or_else(|| {
            AppError::Message(format!(
                "{} sign-in is not configured in this build.",
                provider.display_name()
            ))
        })
    }

    pub fn for_account(provider: &str) -> AppResult<Self> {
        let provider = OAuthProvider::from_id(provider).ok_or_else(|| {
            AppError::Message("This account has an unsupported sign-in provider.".into())
        })?;
        Self::for_provider(provider)
    }

    pub fn provider(&self) -> OAuthProvider {
        match self {
            Self::Google(_) => OAuthProvider::Google,
            Self::Microsoft(_) => OAuthProvider::Microsoft,
        }
    }

    pub fn display_name(&self) -> &'static str {
        self.provider().display_name()
    }

    pub fn bind_address(&self) -> &'static str {
        match self {
            Self::Google(_) => "127.0.0.1:0",
            // Microsoft registers http://localhost. Binding dual-stack keeps
            // both IPv4 and IPv6 localhost resolutions working.
            Self::Microsoft(_) => "[::]:0",
        }
    }

    pub fn redirect_uri(&self, port: u16) -> String {
        match self {
            Self::Google(_) => format!("http://127.0.0.1:{port}"),
            Self::Microsoft(_) => format!("http://localhost:{port}"),
        }
    }

    pub fn authorization_url(
        &self,
        redirect_uri: &str,
        state: &str,
        code_challenge: &str,
    ) -> String {
        match self {
            Self::Google(config) => {
                google::authorization_url(config, redirect_uri, state, code_challenge)
            }
            Self::Microsoft(config) => {
                microsoft::authorization_url(config, redirect_uri, state, code_challenge)
            }
        }
    }

    pub fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        code_verifier: &str,
    ) -> AppResult<TokenGrant> {
        match self {
            Self::Google(config) => {
                google::exchange_code(config, code, redirect_uri, code_verifier)
            }
            Self::Microsoft(config) => {
                microsoft::exchange_code(config, code, redirect_uri, code_verifier)
            }
        }
    }

    pub fn fetch_profile(&self, access_token: &str) -> AppResult<OAuthProfile> {
        match self {
            Self::Google(config) => {
                let profile = google::fetch_profile(config, access_token)?;
                Ok(OAuthProfile {
                    email: profile.email.clone(),
                    username: profile.email,
                    name: profile.name,
                    email_verified: profile.email_verified,
                })
            }
            Self::Microsoft(config) => {
                let profile = microsoft::fetch_profile(config, access_token)?;
                let email = profile.email().ok_or_else(|| {
                    AppError::Message("Microsoft did not return an email address.".into())
                })?;
                Ok(OAuthProfile {
                    email: email.to_string(),
                    username: profile.username().unwrap_or(email).to_string(),
                    name: profile.display_name,
                    email_verified: true,
                })
            }
        }
    }

    pub fn finalize_grant(
        &self,
        grant: TokenGrant,
        profile: OAuthProfile,
    ) -> AppResult<OAuthGrant> {
        let refresh_token = grant.refresh_token.ok_or_else(|| {
            AppError::Message(format!(
                "{} did not return a reusable authorization. Try again.",
                self.display_name()
            ))
        })?;

        match self {
            Self::Google(_) => Ok(OAuthGrant {
                provider: OAuthProvider::Google,
                access_token: grant.access_token,
                refresh_token,
                expires_in: grant.expires_in,
                profile,
            }),
            Self::Microsoft(config) => {
                let outlook = microsoft::exchange_for_outlook(config, &refresh_token)?;
                Ok(OAuthGrant {
                    provider: OAuthProvider::Microsoft,
                    access_token: outlook.access_token,
                    refresh_token: outlook.refresh_token.unwrap_or(refresh_token),
                    expires_in: outlook.expires_in,
                    profile,
                })
            }
        }
    }

    pub fn refresh_access_token(&self, refresh_token: &str) -> AppResult<TokenGrant> {
        match self {
            Self::Google(config) => google::refresh_access_token(config, refresh_token),
            Self::Microsoft(config) => microsoft::refresh_access_token(config, refresh_token),
        }
    }

    pub fn revoke(&self, token: &str) -> AppResult<()> {
        match self {
            Self::Google(config) => google::revoke(config, token),
            Self::Microsoft(config) => microsoft::revoke(config, token),
        }
    }

    pub fn reauth_message(&self) -> &'static str {
        match self {
            Self::Google(_) => "Google authorization expired. Reconnect this account.",
            Self::Microsoft(_) => microsoft::reauth_message(),
        }
    }

    pub fn callback_error(
        &self,
        error: Option<&str>,
        description: Option<&str>,
    ) -> AppError {
        match self {
            Self::Google(_) => {
                if error == Some("access_denied") {
                    AppError::Message("Google sign-in was cancelled.".into())
                } else {
                    AppError::Message("Google sign-in could not be verified. Try again.".into())
                }
            }
            Self::Microsoft(_) => {
                if microsoft::is_admin_consent_error(error, description) {
                    return AppError::Message(microsoft::admin_approval_message().into());
                }
                if error == Some("access_denied")
                    || microsoft::is_user_declined(description)
                {
                    return AppError::Message("Microsoft sign-in was cancelled.".into());
                }
                AppError::Message("Microsoft sign-in could not be verified. Try again.".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{OAuthClientConfig, OAuthProvider, OAuthProfile};
    use crate::oauth::microsoft::MicrosoftClientConfig;
    use crate::oauth::token::TokenGrant;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn provider_ids_and_legacy_google_fallback() {
        assert_eq!(OAuthProvider::from_id("google"), Some(OAuthProvider::Google));
        assert_eq!(OAuthProvider::from_id(""), Some(OAuthProvider::Google));
        assert_eq!(
            OAuthProvider::from_id("microsoft"),
            Some(OAuthProvider::Microsoft)
        );
        assert_eq!(OAuthProvider::from_id("other"), None);
    }

    #[test]
    fn google_finalization_preserves_the_initial_tokens() {
        let config = OAuthClientConfig::Google(crate::oauth::google::GoogleClientConfig {
            client_id: "client-id".into(),
            client_secret: None,
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            userinfo_url: "https://openidconnect.googleapis.com/v1/userinfo".into(),
            revoke_url: "https://oauth2.googleapis.com/revoke".into(),
            scopes: "openid".into(),
        });
        let grant = TokenGrant {
            access_token: "access".into(),
            refresh_token: Some("refresh".into()),
            expires_in: 3600,
        };
        let profile = super::OAuthProfile {
            email: "me@example.com".into(),
            username: "me@example.com".into(),
            name: Some("Me".into()),
            email_verified: true,
        };
        let finalized = config.finalize_grant(grant, profile).unwrap();
        assert_eq!(finalized.provider, OAuthProvider::Google);
        assert_eq!(finalized.access_token, "access");
        assert_eq!(finalized.refresh_token, "refresh");
    }

    #[test]
    fn microsoft_finalization_stores_outlook_tokens() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = runtime.block_on(async {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "outlook-access",
                    "refresh_token": "outlook-refresh",
                    "expires_in": 1800,
                })))
                .mount(&server)
                .await;
            server
        });

        let config = OAuthClientConfig::Microsoft(MicrosoftClientConfig {
            client_id: "client-id".into(),
            auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
            token_url: format!("{}/token", server.uri()),
            profile_url: "https://graph.microsoft.com/v1.0/me".into(),
            scopes: vec!["openid".into()],
            outlook_scopes: vec![
                "offline_access".into(),
                "https://outlook.office.com/IMAP.AccessAsUser.All".into(),
                "https://outlook.office.com/SMTP.Send".into(),
            ],
        });
        let profile = OAuthProfile {
            email: "nicole@contoso.com".into(),
            username: "nicole@contoso.onmicrosoft.com".into(),
            name: Some("Nicole".into()),
            email_verified: true,
        };
        let finalized = config
            .finalize_grant(
                TokenGrant {
                    access_token: "graph-access".into(),
                    refresh_token: Some("graph-refresh".into()),
                    expires_in: 3600,
                },
                profile,
            )
            .unwrap();

        assert_eq!(finalized.provider, OAuthProvider::Microsoft);
        assert_eq!(finalized.access_token, "outlook-access");
        assert_eq!(finalized.refresh_token, "outlook-refresh");
        assert_eq!(finalized.expires_in, 1800);
        assert_eq!(finalized.profile.username, "nicole@contoso.onmicrosoft.com");

        let requests = runtime.block_on(server.received_requests()).unwrap();
        let body = String::from_utf8(requests[0].body.clone()).unwrap();
        assert!(body.contains("grant_type=refresh_token"));
        assert!(body.contains("graph-refresh"));
        assert!(body.contains("IMAP.AccessAsUser.All"));
        assert!(body.contains("SMTP.Send"));
    }
}
