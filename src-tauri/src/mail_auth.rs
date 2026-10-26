use crate::accounts::{Account, AccountAuth};
use crate::error::AppResult;
use crate::secrets::{account_key, SecretStore};

#[derive(Clone, PartialEq, Eq)]
pub enum MailCredential {
    Password(String),
    AccessToken(String),
}

impl std::fmt::Debug for MailCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password(_) => formatter.write_str("Password(***)"),
            Self::AccessToken(_) => formatter.write_str("AccessToken(***)"),
        }
    }
}

impl MailCredential {
    pub fn secret(&self) -> &str {
        match self {
            Self::Password(value) | Self::AccessToken(value) => value,
        }
    }

    pub fn is_oauth(&self) -> bool {
        matches!(self, Self::AccessToken(_))
    }
}

pub fn credential_for(
    account: &Account,
    secrets: &dyn SecretStore,
) -> AppResult<MailCredential> {
    if account.auth_type == AccountAuth::Proton {
        crate::proton::runtime::current()?.start_if_needed()?;
        return Ok(MailCredential::Password(
            secrets.get(&account_key(account.id))?,
        ));
    }
    crate::oauth::credential_for(account, secrets)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use tempfile::TempDir;

    use super::*;
    use crate::accounts::{self, ProtonAccountInput, SignatureFields};
    use crate::db::Database;
    use crate::proton::{runtime, ProtonAccessLimited, ProtonBridge, ProtonEvents};
    use crate::secrets::MemoryStore;

    struct NoEvents;

    impl ProtonEvents for NoEvents {
        fn access_limited(&self, _status: ProtonAccessLimited) {}
    }

    struct Fixture {
        dir: TempDir,
        db: Arc<Database>,
        secrets: Arc<MemoryStore>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                dir: tempfile::tempdir().unwrap(),
                db: Arc::new(Database::open_memory().unwrap()),
                secrets: Arc::new(MemoryStore::default()),
            }
        }

        fn config_dir(&self) -> PathBuf {
            self.dir.path().join("proton-bridge")
        }

        fn install(&self) -> Arc<ProtonBridge> {
            let bridge = Arc::new(ProtonBridge::new(
                Arc::new(NoEvents),
                Arc::clone(&self.db),
                Arc::clone(&self.secrets) as Arc<dyn SecretStore>,
                self.config_dir(),
                sidecar(),
            ));
            runtime::install(Arc::clone(&bridge));
            bridge
        }

        fn add_proton(&self, email: &str) -> Account {
            let ports = crate::proton::ports::free_pair().unwrap();
            accounts::create_proton(
                &self.db,
                self.secrets.as_ref(),
                ProtonAccountInput {
                    display_name: "Proton".into(),
                    email: email.into(),
                    username: email.into(),
                    bridge_password: "bridge-password".into(),
                    imap_host: "127.0.0.1".into(),
                    imap_port: ports.imap,
                    smtp_host: "127.0.0.1".into(),
                    smtp_port: ports.smtp,
                    color: "lavender".into(),
                    signature: String::new(),
                    signature_fields: SignatureFields::default(),
                    space_id: crate::spaces::default_id(&self.db).unwrap(),
                },
            )
            .unwrap()
        }

        fn generation(&self) -> Option<String> {
            fs::read_to_string(self.config_dir().join("sidecar-generation")).ok()
        }
    }

    fn sidecar() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".local-tests/proton-bridge/fake-sidecar.sh")
    }

    fn account(auth_type: AccountAuth, id: i64) -> Account {
        Account {
            id,
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
            auth_type,
            oauth_provider: String::new(),
            space_id: 1,
            position: 0,
            sync_interval_secs: 120,
            created_at: 0,
        }
    }

    #[test]
    fn proton_accounts_use_the_bridge_password_and_start_the_bridge() {
        let _guard = runtime::test_guard();
        let fixture = Fixture::new();
        let bridge = fixture.install();
        let proton = fixture.add_proton("me@proton.me");

        let credential = credential_for(&proton, fixture.secrets.as_ref()).unwrap();

        assert_eq!(
            credential,
            MailCredential::Password("bridge-password".into())
        );
        assert!(bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("1"));
        runtime::stop();
    }

    #[test]
    fn password_accounts_do_not_touch_the_bridge() {
        let _guard = runtime::test_guard();
        let fixture = Fixture::new();
        let bridge = fixture.install();
        let password = account(AccountAuth::Password, 42);
        fixture.secrets.set(&account_key(42), "p@ss").unwrap();

        let credential = credential_for(&password, fixture.secrets.as_ref()).unwrap();

        assert_eq!(credential, MailCredential::Password("p@ss".into()));
        assert!(!bridge.is_running());
        assert_eq!(fixture.generation(), None);
        runtime::stop();
    }

    #[test]
    fn oauth_accounts_still_go_through_the_oauth_credential_flow() {
        let _guard = runtime::test_guard();
        let fixture = Fixture::new();
        fixture.install();
        let mut oauth = account(AccountAuth::OAuth, 7);
        oauth.oauth_provider = "unknown-provider".into();

        let error = credential_for(&oauth, fixture.secrets.as_ref()).unwrap_err();

        assert_eq!(
            error.to_string(),
            "This account has an unsupported sign-in provider."
        );
        runtime::stop();
    }

    #[test]
    fn startup_does_not_spawn_the_sidecar_without_proton_accounts() {
        let _guard = runtime::test_guard();
        let fixture = Fixture::new();
        let bridge = fixture.install();

        runtime::start_if_needed(&fixture.db).unwrap();

        assert!(!bridge.is_running());
        assert_eq!(fixture.generation(), None);
        runtime::stop();
    }

    #[test]
    fn startup_starts_the_sidecar_for_a_proton_account() {
        let _guard = runtime::test_guard();
        let fixture = Fixture::new();
        let bridge = fixture.install();
        fixture.add_proton("me@proton.me");

        runtime::start_if_needed(&fixture.db).unwrap();

        assert!(bridge.is_running());
        assert_eq!(fixture.generation().as_deref(), Some("1"));
        runtime::stop();
    }
}
