use std::sync::Mutex;

use crate::accounts::{Account, AccountAuth};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::mail_auth::MailCredential;
use crate::secrets::{account_key, SecretStore, SECRET_NOT_FOUND};

pub mod account;
pub mod flow;
pub mod google;
pub mod microsoft;
pub mod provider;
pub mod secret;
pub mod token;

use self::provider::OAuthClientConfig;
use self::secret::OAuthSecret;

static REFRESH_LOCK: Mutex<()> = Mutex::new(());

pub fn credential_for(
    account: &Account,
    secrets: &dyn SecretStore,
) -> AppResult<MailCredential> {
    match account.auth_type {
        AccountAuth::Password => Ok(MailCredential::Password(
            secrets.get(&account_key(account.id))?,
        )),
        AccountAuth::OAuth => {
            let config = OAuthClientConfig::for_account(&account.oauth_provider)?;
            credential_for_with_config(account, secrets, &config)
        }
        AccountAuth::Proton => Err(AppError::Message(
            "Proton bridge is not available yet".into(),
        )),
    }
}

pub fn credential_for_with_config(
    account: &Account,
    secrets: &dyn SecretStore,
    config: &OAuthClientConfig,
) -> AppResult<MailCredential> {
    match account.auth_type {
        AccountAuth::Password => Ok(MailCredential::Password(
            secrets.get(&account_key(account.id))?,
        )),
        AccountAuth::OAuth => oauth_credential(account, secrets, config),
        AccountAuth::Proton => Err(AppError::Message(
            "Proton bridge is not available yet".into(),
        )),
    }
}

fn oauth_credential(
    account: &Account,
    secrets: &dyn SecretStore,
    config: &OAuthClientConfig,
) -> AppResult<MailCredential> {
    let _guard = REFRESH_LOCK
        .lock()
        .map_err(|_| AppError::Message("secret store unavailable".into()))?;

    let key = account_key(account.id);
    let encoded = match secrets.get(&key) {
        Ok(value) => value,
        Err(AppError::Message(message)) if message.as_str() == SECRET_NOT_FOUND => {
            return Err(reauth_error(config));
        }
        Err(error) => return Err(error),
    };
    let mut secret = OAuthSecret::parse(&encoded).map_err(|_| reauth_error(config))?;
    if secret.refresh_token.is_empty() {
        return Err(reauth_error(config));
    }

    let now = db::now();
    if secret.is_fresh(now) {
        return Ok(MailCredential::AccessToken(secret.access_token));
    }

    let grant = config.refresh_access_token(&secret.refresh_token)?;
    secret.apply_grant(&grant, now);

    // Best-effort: persist the refreshed token. If the keyring write fails we
    // still return the fresh access token, and the refresh token is unchanged.
    if let Ok(updated) = secret.encode() {
        let _ = secrets.set(&key, &updated);
    }

    Ok(MailCredential::AccessToken(secret.access_token))
}

fn reauth_error(config: &OAuthClientConfig) -> AppError {
    AppError::Reauth(config.reauth_message().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::AccountAuth;
    use crate::oauth::google::GoogleClientConfig;
    use crate::oauth::provider::OAuthClientConfig;
    use crate::oauth::secret::OAuthSecret;
    use crate::oauth::token::TokenGrant;
    use crate::secrets::{account_key, MemoryStore};
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn account_with_auth(auth: AccountAuth) -> Account {
        Account {
            id: 1,
            display_name: "Me".into(),
            email: "me@example.com".into(),
            username: "me@example.com".into(),
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            imap_security: "tls".into(),
            smtp_host: "smtp.example.com".into(),
            smtp_port: 465,
            smtp_security: "tls".into(),
            color: "accent".into(),
            signature: String::new(),
            signature_fields: Default::default(),
            signature_on_new: true,
            auth_type: auth,
            oauth_provider: String::new(),
            space_id: 1,
            position: 0,
            sync_interval_secs: 120,
            created_at: 0,
        }
    }

    fn test_config(token_url: &str) -> OAuthClientConfig {
        OAuthClientConfig::Google(GoogleClientConfig {
            client_id: "client-id".into(),
            client_secret: Some("client-secret".into()),
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: token_url.to_string(),
            userinfo_url: "https://openidconnect.googleapis.com/v1/userinfo".into(),
            revoke_url: "https://oauth2.googleapis.com/revoke".into(),
            scopes: "openid email profile https://mail.google.com/".into(),
        })
    }

    #[test]
    fn password_accounts_read_the_keyring_secret() {
        let secrets = MemoryStore::default();
        let account = account_with_auth(AccountAuth::Password);
        secrets.set(&account_key(account.id), "p@ss").unwrap();
        assert_eq!(
            credential_for(&account, &secrets).unwrap(),
            MailCredential::Password("p@ss".into())
        );
    }

    #[test]
    fn credential_debug_output_redacts_the_secret() {
        let credential = MailCredential::AccessToken("token-123".into());
        assert!(!format!("{credential:?}").contains("token-123"));
    }

    #[test]
    fn expired_secret_refreshes_and_updates_the_keyring() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = runtime.block_on(async {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "new-token",
                    "expires_in": 3600,
                })))
                .mount(&server)
                .await;
            server
        });

        let config = test_config(&format!("{}/token", server.uri()));
        let secrets = MemoryStore::default();
        let account = account_with_auth(AccountAuth::OAuth);
        let grant = TokenGrant {
            access_token: "old-token".into(),
            refresh_token: Some("refresh-token".into()),
            expires_in: 0,
        };
        let secret = OAuthSecret::new("google", &grant, 0).unwrap();
        secrets
            .set(&account_key(account.id), &secret.encode().unwrap())
            .unwrap();

        let credential = credential_for_with_config(&account, &secrets, &config).unwrap();
        assert_eq!(credential, MailCredential::AccessToken("new-token".into()));

        let stored =
            OAuthSecret::parse(&secrets.get(&account_key(account.id)).unwrap()).unwrap();
        assert_eq!(stored.access_token, "new-token");
        assert_eq!(stored.refresh_token, "refresh-token");
    }

    #[test]
    fn invalid_grant_maps_to_reauth() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = runtime.block_on(async {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                    "error": "invalid_grant",
                })))
                .mount(&server)
                .await;
            server
        });

        let config = test_config(&format!("{}/token", server.uri()));
        let secrets = MemoryStore::default();
        let account = account_with_auth(AccountAuth::OAuth);
        let grant = TokenGrant {
            access_token: "old-token".into(),
            refresh_token: Some("refresh-token".into()),
            expires_in: 0,
        };
        let secret = OAuthSecret::new("google", &grant, 0).unwrap();
        secrets
            .set(&account_key(account.id), &secret.encode().unwrap())
            .unwrap();

        let error = credential_for_with_config(&account, &secrets, &config).unwrap_err();
        assert!(error.is_reauth());
        assert_eq!(
            error.to_string(),
            "Google authorization expired. Reconnect this account."
        );
    }

    #[test]
    fn corrupt_or_tokenless_secret_returns_reauth() {
        let config = test_config("https://oauth2.googleapis.com/token");
        let secrets = MemoryStore::default();
        let account = account_with_auth(AccountAuth::OAuth);

        secrets.set(&account_key(account.id), "not json").unwrap();
        let error = credential_for_with_config(&account, &secrets, &config).unwrap_err();
        assert!(error.is_reauth());

        secrets
            .set(
                &account_key(account.id),
                r#"{"version":1,"provider":"google","refresh_token":"","access_token":"","expires_at":0}"#,
            )
            .unwrap();
        let error = credential_for_with_config(&account, &secrets, &config).unwrap_err();
        assert!(error.is_reauth());
    }

    #[test]
    fn missing_oauth_keyring_entry_returns_reauth() {
        let config = test_config("https://oauth2.googleapis.com/token");
        let secrets = MemoryStore::default();
        let account = account_with_auth(AccountAuth::OAuth);

        let error = credential_for_with_config(&account, &secrets, &config).unwrap_err();
        assert!(error.is_reauth());
        assert_eq!(
            error.to_string(),
            "Google authorization expired. Reconnect this account."
        );
    }
}
