use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::accounts::{self, Account, AccountAuth, MailSecurity, OAuthAccountInput};
use crate::db::{self, Database};
use crate::error::{AppError, AppResult};
use crate::imap::client::validate_imap;
use crate::mail_auth::MailCredential;
use crate::oauth::flow::{self, OAuthFlowState};
use crate::oauth::provider::{OAuthClientConfig, OAuthGrant, OAuthProfile, OAuthProvider};
use crate::oauth::secret::OAuthSecret;
use crate::oauth::token::TokenGrant;
use crate::secrets::{account_key, SecretStore};
use crate::smtp::validate_smtp;
use crate::spaces;

const ACCOUNT_COLORS: [&str; 10] = [
    "blue",
    "lavender",
    "mint",
    "peach",
    "rose",
    "sand",
    "sage",
    "sky",
    "lilac",
    "slate",
];

/// RAII guard that runs a cleanup closure on drop unless disarmed. Used to
/// guarantee `flow.finish` and best-effort token revocation on every early
/// error path after a grant is obtained.
struct CleanupGuard<F: FnMut()> {
    cleanup: F,
    armed: bool,
}

impl<F: FnMut()> CleanupGuard<F> {
    fn new(cleanup: F) -> Self {
        Self {
            cleanup,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl<F: FnMut()> Drop for CleanupGuard<F> {
    fn drop(&mut self) {
        if self.armed {
            (self.cleanup)();
        }
    }
}

/// Build a cleanup closure that finishes the active flow and revokes the
/// refresh token best-effort.
fn grant_cleanup<'a>(
    flow: &'a OAuthFlowState,
    cancel: &Arc<std::sync::atomic::AtomicBool>,
    config: &OAuthClientConfig,
    refresh_token: &str,
) -> CleanupGuard<impl FnMut() + 'a> {
    let flow = flow;
    let cancel = Arc::clone(cancel);
    let config = config.clone();
    let refresh_token = refresh_token.to_string();
    CleanupGuard::new(move || {
        flow.finish(&cancel);
        let _ = config.revoke(&refresh_token);
    })
}

fn cancelled(config: &OAuthClientConfig) -> AppError {
    AppError::Message(format!("{} sign-in was cancelled.", config.display_name()))
}

fn identity_mismatch(provider: OAuthProvider) -> AppError {
    AppError::Message(format!(
        "Sign in with the {} account already connected here.",
        provider.display_name()
    ))
}

/// Start the Google sign-in browser flow and create a new Gmail OAuth account.
pub fn start_google_oauth(
    db: &Database,
    secrets: &dyn SecretStore,
    flow: &OAuthFlowState,
    space_id: i64,
    on_phase: &dyn Fn(&str, Option<&str>),
) -> AppResult<Account> {
    start_oauth(
        db,
        secrets,
        flow,
        OAuthClientConfig::for_provider(OAuthProvider::Google)?,
        space_id,
        on_phase,
    )
}

/// Start the Microsoft sign-in browser flow and create a new Outlook OAuth
/// account.
pub fn start_microsoft_oauth(
    db: &Database,
    secrets: &dyn SecretStore,
    flow: &OAuthFlowState,
    space_id: i64,
    on_phase: &dyn Fn(&str, Option<&str>),
) -> AppResult<Account> {
    start_oauth(
        db,
        secrets,
        flow,
        OAuthClientConfig::for_provider(OAuthProvider::Microsoft)?,
        space_id,
        on_phase,
    )
}

/// On any failure after a grant is obtained, revoke the refresh token
/// best-effort where the provider supports it, so it cannot outlive the
/// abandoned sign-in.
fn start_oauth(
    db: &Database,
    secrets: &dyn SecretStore,
    flow: &OAuthFlowState,
    config: OAuthClientConfig,
    space_id: i64,
    on_phase: &dyn Fn(&str, Option<&str>),
) -> AppResult<Account> {
    spaces::by_id(db, space_id)?;
    let cancel = flow.begin(config.provider())?;

    let grant = match flow::authorize(config.clone(), Arc::clone(&cancel), on_phase) {
        Ok(grant) => grant,
        Err(error) => {
            flow.finish(&cancel);
            return Err(error);
        }
    };

    let mut cleanup = grant_cleanup(flow, &cancel, &config, &grant.refresh_token);

    if accounts::oauth_identity_taken(
        db,
        &grant.profile.email,
        config.provider().id(),
    )? {
        return Err(AppError::Message("This account is already added.".into()));
    }

    let mut input = account_input(config.provider(), &grant.profile, space_id);

    if cancel.load(Ordering::SeqCst) {
        return Err(cancelled(&config));
    }

    let credential = MailCredential::AccessToken(grant.access_token.clone());
    validate_servers(config.provider(), &input.username, &credential)?;

    input.color = next_account_color(db)?.to_string();

    if cancel.load(Ordering::SeqCst) {
        return Err(cancelled(&config));
    }

    let secret = encode_grant_secret(&grant)?;
    let account = accounts::create_oauth(db, secrets, input, &secret)?;

    cleanup.disarm();
    flow.finish(&cancel);
    Ok(account)
}

/// Re-run the Google browser flow and replace only the keyring secret for an
/// existing Google OAuth account.
pub fn reconnect_google_account(
    db: &Database,
    secrets: &dyn SecretStore,
    flow: &OAuthFlowState,
    account_id: i64,
    on_phase: &dyn Fn(&str, Option<&str>),
) -> AppResult<Account> {
    reconnect_oauth(
        db,
        secrets,
        flow,
        OAuthProvider::Google,
        account_id,
        on_phase,
    )
}

/// Re-run the Microsoft browser flow and replace only the keyring secret for an
/// existing Microsoft OAuth account.
pub fn reconnect_microsoft_account(
    db: &Database,
    secrets: &dyn SecretStore,
    flow: &OAuthFlowState,
    account_id: i64,
    on_phase: &dyn Fn(&str, Option<&str>),
) -> AppResult<Account> {
    reconnect_oauth(
        db,
        secrets,
        flow,
        OAuthProvider::Microsoft,
        account_id,
        on_phase,
    )
}

fn reconnect_oauth(
    db: &Database,
    secrets: &dyn SecretStore,
    flow: &OAuthFlowState,
    provider: OAuthProvider,
    account_id: i64,
    on_phase: &dyn Fn(&str, Option<&str>),
) -> AppResult<Account> {
    let existing = accounts::list(db)?
        .into_iter()
        .find(|account| account.id == account_id)
        .ok_or_else(|| AppError::Message("account not found".into()))?;
    if existing.auth_type != AccountAuth::OAuth
        || OAuthProvider::from_id(&existing.oauth_provider) != Some(provider)
    {
        return Err(AppError::Message(format!(
            "This account does not use {} sign-in.",
            provider.display_name()
        )));
    }

    let config = OAuthClientConfig::for_provider(provider)?;
    let cancel = flow.begin(provider)?;

    let grant = match flow::authorize(config.clone(), Arc::clone(&cancel), on_phase) {
        Ok(grant) => grant,
        Err(error) => {
            flow.finish(&cancel);
            return Err(error);
        }
    };

    let mut cleanup = grant_cleanup(flow, &cancel, &config, &grant.refresh_token);

    let current = match accounts::list(db)?
        .into_iter()
        .find(|account| account.id == account_id)
    {
        Some(account) => account,
        None => return Err(AppError::Message("account not found".into())),
    };

    check_identity(provider, &current.email, &grant.profile.email)?;

    if cancel.load(Ordering::SeqCst) {
        return Err(cancelled(&config));
    }

    let credential = MailCredential::AccessToken(grant.access_token.clone());
    validate_servers(provider, &current.username, &credential)?;

    if cancel.load(Ordering::SeqCst) {
        return Err(cancelled(&config));
    }

    let secret = encode_grant_secret(&grant)?;
    secrets.set(&account_key(account_id), &secret)?;

    cleanup.disarm();
    flow.finish(&cancel);
    Ok(current)
}

/// Compare the stored provider email against the one returned by the fresh
/// authorization, case-insensitively.
pub fn check_identity(
    provider: OAuthProvider,
    expected: &str,
    actual: &str,
) -> AppResult<()> {
    if expected.trim().eq_ignore_ascii_case(actual.trim()) {
        Ok(())
    } else {
        Err(identity_mismatch(provider))
    }
}

/// Compare the stored Google email against the one returned by the fresh
/// authorization, case-insensitively.
#[cfg(test)]
pub fn check_google_identity(expected: &str, actual: &str) -> AppResult<()> {
    check_identity(OAuthProvider::Google, expected, actual)
}

/// Compare the stored Microsoft email against the one returned by the fresh
/// authorization, case-insensitively.
#[cfg(test)]
pub fn check_microsoft_identity(expected: &str, actual: &str) -> AppResult<()> {
    check_identity(OAuthProvider::Microsoft, expected, actual)
}

/// Best-effort read of the refresh token from the keyring, for deletion-time
/// revocation. Must be called before the keyring entry is deleted.
pub fn oauth_refresh_token(account: &Account, secrets: &dyn SecretStore) -> Option<String> {
    let encoded = secrets.get(&account_key(account.id)).ok()?;
    let secret = OAuthSecret::parse(&encoded).ok()?;
    (!secret.refresh_token.is_empty()).then_some(secret.refresh_token)
}

/// Best-effort revocation of a token for an account's provider. Microsoft has
/// no simple revocation endpoint, so this is a no-op there.
pub fn revoke_account_token(account: &Account, token: &str) {
    if let Ok(config) = OAuthClientConfig::for_account(&account.oauth_provider) {
        let _ = config.revoke(token);
    }
}

fn account_input(
    provider: OAuthProvider,
    profile: &OAuthProfile,
    space_id: i64,
) -> OAuthAccountInput {
    let display_name = profile
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            profile
                .email
                .split('@')
                .next()
                .unwrap_or(&profile.email)
                .to_string()
        });
    let (imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, provider_id) =
        match provider {
            OAuthProvider::Google => (
                "imap.gmail.com",
                993,
                "tls",
                "smtp.gmail.com",
                465,
                "tls",
                "google",
            ),
            OAuthProvider::Microsoft => (
                "outlook.office365.com",
                993,
                "tls",
                "smtp.office365.com",
                587,
                "starttls",
                "microsoft",
            ),
        };
    OAuthAccountInput {
        display_name,
        email: profile.email.clone(),
        username: profile.username.clone(),
        imap_host: imap_host.into(),
        imap_port,
        imap_security: imap_security.into(),
        smtp_host: smtp_host.into(),
        smtp_port,
        smtp_security: smtp_security.into(),
        color: String::new(),
        provider: provider_id.into(),
        space_id,
    }
}

fn encode_grant_secret(grant: &OAuthGrant) -> AppResult<String> {
    let token = TokenGrant {
        access_token: grant.access_token.clone(),
        refresh_token: Some(grant.refresh_token.clone()),
        expires_in: grant.expires_in,
    };
    OAuthSecret::new(grant.provider.id(), &token, db::now())?.encode()
}

fn validate_servers(
    provider: OAuthProvider,
    email: &str,
    credential: &MailCredential,
) -> AppResult<()> {
    match provider {
        OAuthProvider::Google => {
            validate_imap("imap.gmail.com", 993, MailSecurity::Tls, email, credential)?;
            validate_smtp("smtp.gmail.com", 465, MailSecurity::Tls, email, credential)?;
        }
        OAuthProvider::Microsoft => {
            validate_imap(
                "outlook.office365.com",
                993,
                MailSecurity::Tls,
                email,
                credential,
            )?;
            validate_smtp(
                "smtp.office365.com",
                587,
                MailSecurity::StartTls,
                email,
                credential,
            )?;
        }
    }
    Ok(())
}

fn next_account_color(db: &Database) -> AppResult<&'static str> {
    let count = accounts::list(db)?.len();
    Ok(ACCOUNT_COLORS[count % ACCOUNT_COLORS.len()])
}

#[cfg(test)]
mod tests {
    use super::{
        account_input, check_google_identity, check_microsoft_identity, CleanupGuard,
    };
    use crate::oauth::provider::{OAuthProvider, OAuthProfile};

    #[test]
    fn reconnect_rejects_a_different_google_email() {
        let error = check_google_identity("me@gmail.com", "other@gmail.com").unwrap_err();
        assert_eq!(
            error.to_string(),
            "Sign in with the Google account already connected here."
        );
        assert!(check_google_identity("Me@Gmail.com", "me@gmail.com").is_ok());
    }

    #[test]
    fn reconnect_rejects_a_different_microsoft_email() {
        let error = check_microsoft_identity("me@outlook.com", "other@outlook.com").unwrap_err();
        assert_eq!(
            error.to_string(),
            "Sign in with the Microsoft account already connected here."
        );
        assert!(check_microsoft_identity("Me@Outlook.com", "me@outlook.com").is_ok());
    }

    #[test]
    fn microsoft_account_uses_outlook_servers_and_upn_username() {
        let profile = OAuthProfile {
            email: "nicole@contoso.com".into(),
            username: "nicole@contoso.onmicrosoft.com".into(),
            name: Some("Nicole".into()),
            email_verified: true,
        };
        let input = account_input(OAuthProvider::Microsoft, &profile, 7);
        assert_eq!(input.email, "nicole@contoso.com");
        assert_eq!(input.username, "nicole@contoso.onmicrosoft.com");
        assert_eq!(input.imap_host, "outlook.office365.com");
        assert_eq!(input.imap_port, 993);
        assert_eq!(input.imap_security, "tls");
        assert_eq!(input.smtp_host, "smtp.office365.com");
        assert_eq!(input.smtp_port, 587);
        assert_eq!(input.smtp_security, "starttls");
        assert_eq!(input.provider, "microsoft");
        assert_eq!(input.space_id, 7);
        assert_eq!(input.display_name, "Nicole");
    }

    #[test]
    fn cleanup_guard_runs_unless_disarmed() {
        use std::cell::Cell;

        let fired = Cell::new(false);
        {
            let _guard = CleanupGuard::new(|| fired.set(true));
        }
        assert!(fired.get(), "cleanup must run when the guard is not disarmed");

        fired.set(false);
        {
            let mut guard = CleanupGuard::new(|| fired.set(true));
            guard.disarm();
        }
        assert!(!fired.get(), "cleanup must not run after disarm");
    }
}
