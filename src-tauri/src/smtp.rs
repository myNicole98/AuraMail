use std::time::Duration;

use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{SmtpTransport, Transport};

use crate::accounts::{Account, AccountAuth, MailSecurity};
use crate::compose::{self, ComposeInput};
use crate::error::{AppError, AppResult};
use crate::imap::client::proton_host_allowed;
use crate::mail_auth::MailCredential;

const TIMEOUT: Duration = Duration::from_secs(10);
const SEND_TIMEOUT: Duration = Duration::from_secs(60);

fn smtp_mechanisms(credential: &MailCredential) -> Option<Vec<Mechanism>> {
    credential.is_oauth().then(|| vec![Mechanism::Xoauth2])
}

fn transport(
    host: &str,
    port: u16,
    security: MailSecurity,
    username: &str,
    credential: &MailCredential,
    timeout: Duration,
) -> AppResult<SmtpTransport> {
    if host.trim().is_empty() {
        return Err(AppError::Message("SMTP host is required".into()));
    }
    if port == 0 {
        return Err(AppError::Message("SMTP port is required".into()));
    }

    let tls_parameters = TlsParameters::new(host.to_string())
        .map_err(|_| AppError::Message("SMTP TLS configuration failed".into()))?;
    let tls = match security {
        MailSecurity::Tls => Tls::Wrapper(tls_parameters),
        MailSecurity::StartTls => Tls::Required(tls_parameters),
    };
    let mut builder = SmtpTransport::builder_dangerous(host)
        .port(port)
        .tls(tls)
        .credentials(Credentials::new(username.into(), credential.secret().into()))
        .timeout(Some(timeout));
    if let Some(mechanisms) = smtp_mechanisms(credential) {
        builder = builder.authentication(mechanisms);
    }
    Ok(builder.build())
}

/// Proton Bridge accepts plaintext only on loopback, so no TLS is negotiated.
pub fn proton_smtp_transport(
    host: &str,
    port: u16,
    username: &str,
    credential: &MailCredential,
    timeout: Duration,
) -> AppResult<SmtpTransport> {
    if host.trim().is_empty() {
        return Err(AppError::Message("SMTP host is required".into()));
    }
    if port == 0 {
        return Err(AppError::Message("SMTP port is required".into()));
    }
    if !proton_host_allowed(host) {
        return Err(AppError::Message(
            "Proton Bridge must be reached over loopback".into(),
        ));
    }

    let mut builder = SmtpTransport::builder_dangerous(host)
        .port(port)
        .tls(Tls::None)
        .credentials(Credentials::new(
            username.into(),
            credential.secret().into(),
        ))
        .timeout(Some(timeout));
    if let Some(mechanisms) = smtp_mechanisms(credential) {
        builder = builder.authentication(mechanisms);
    }
    Ok(builder.build())
}

fn account_transport(
    account: &Account,
    credential: &MailCredential,
    timeout: Duration,
) -> AppResult<SmtpTransport> {
    if account.auth_type == AccountAuth::Proton {
        return proton_smtp_transport(
            &account.smtp_host,
            account.smtp_port,
            &account.username,
            credential,
            timeout,
        );
    }
    let security = MailSecurity::parse(&account.smtp_security)?;
    transport(
        &account.smtp_host,
        account.smtp_port,
        security,
        &account.username,
        credential,
        timeout,
    )
}

pub fn validate_smtp(
    host: &str,
    port: u16,
    security: MailSecurity,
    username: &str,
    credential: &MailCredential,
) -> AppResult<()> {
    check_transport(transport(
        host, port, security, username, credential, TIMEOUT,
    )?)
}

/// Validates a Proton Bridge SMTP endpoint over plaintext loopback.
#[allow(dead_code)]
pub fn validate_proton_smtp(
    host: &str,
    port: u16,
    username: &str,
    credential: &MailCredential,
) -> AppResult<()> {
    check_transport(proton_smtp_transport(
        host, port, username, credential, TIMEOUT,
    )?)
}

fn check_transport(transport: SmtpTransport) -> AppResult<()> {
    match transport.test_connection() {
        Ok(true) => Ok(()),
        Ok(false) => Err(AppError::Message("SMTP connection failed".into())),
        Err(err) => Err(safe_smtp_error(err)),
    }
}

pub fn send(
    account: &Account,
    credential: &MailCredential,
    input: &ComposeInput,
) -> AppResult<()> {
    let message = compose::build(input, &account.email)?;
    let transport = account_transport(account, credential, SEND_TIMEOUT)?;
    transport.send(&message).map_err(safe_smtp_error)?;
    Ok(())
}

fn safe_smtp_error(err: lettre::transport::smtp::Error) -> AppError {
    if err.is_timeout() {
        return AppError::Message("SMTP connection timed out".into());
    }
    if err.is_tls() {
        return AppError::Message("SMTP TLS handshake failed".into());
    }
    if let Some(code) = err.status() {
        return AppError::Message(status_message(u16::from(code)).into());
    }
    if err.is_transient() {
        return AppError::Message("SMTP server temporarily unavailable".into());
    }
    if err.is_permanent() {
        return AppError::Message("SMTP server rejected the connection".into());
    }
    if err.is_response() {
        return AppError::Message("SMTP server returned an error".into());
    }
    AppError::Message("SMTP connection failed".into())
}

fn status_message(code: u16) -> &'static str {
    match code {
        530 => "SMTP authentication required",
        534 => "SMTP authentication method not supported",
        535 => "SMTP authentication failed",
        552 => "SMTP server rejected the message size",
        400..=499 => "SMTP server temporarily unavailable",
        500..=599 => "SMTP server rejected the connection",
        _ => "SMTP server returned an error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::MailSecurity;

    fn account() -> Account {
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
            auth_type: Default::default(),
            oauth_provider: String::new(),
            space_id: 1,
            position: 0,
            sync_interval_secs: 120,
            created_at: 0,
        }
    }

    fn proton_account() -> Account {
        let mut proton = account();
        proton.auth_type = crate::accounts::AccountAuth::Proton;
        proton.smtp_host = "127.0.0.1".into();
        proton.smtp_port = 1025;
        proton.smtp_security = "plain".into();
        proton
    }

    fn input() -> ComposeInput {
        ComposeInput {
            account_id: 1,
            to: vec!["ada@example.com".into()],
            cc: vec![],
            subject: "Hello".into(),
            body: "Body".into(),
            html_body: None,
            inline_images: vec![],
            in_reply_to: None,
            references: None,
            attachments: vec![],
        }
    }

    #[test]
    fn rejects_blank_host_without_network_access() {
        assert!(validate_smtp(
            "",
            465,
            MailSecurity::Tls,
            "user",
            &MailCredential::Password("pass".into())
        )
        .is_err());
    }

    #[test]
    fn rejects_invalid_port_without_network_access() {
        assert!(validate_smtp(
            "smtp.example.com",
            0,
            MailSecurity::StartTls,
            "user",
            &MailCredential::Password("pass".into())
        )
        .is_err());
    }

    #[test]
    fn send_validates_the_message_before_connecting() {
        let mut message = input();
        message.to = vec!["not-an-email".into()];
        let error = send(
            &account(),
            &MailCredential::Password("secret".into()),
            &message,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "invalid recipient address");
    }

    #[test]
    fn maps_smtp_statuses_to_safe_messages() {
        assert_eq!(status_message(530), "SMTP authentication required");
        assert_eq!(
            status_message(534),
            "SMTP authentication method not supported"
        );
        assert_eq!(status_message(535), "SMTP authentication failed");
        assert_eq!(status_message(552), "SMTP server rejected the message size");
        assert_eq!(status_message(450), "SMTP server temporarily unavailable");
        assert_eq!(status_message(550), "SMTP server rejected the connection");
        assert_eq!(status_message(250), "SMTP server returned an error");
    }

    #[test]
    fn oauth_uses_xoauth2_mechanism() {
        let credential = MailCredential::AccessToken("token-123".into());
        assert_eq!(
            smtp_mechanisms(&credential),
            Some(vec![Mechanism::Xoauth2])
        );
        let password = MailCredential::Password("secret".into());
        assert_eq!(smtp_mechanisms(&password), None);
    }

    #[test]
    fn proton_smtp_transport_accepts_only_loopback_hosts() {
        let credential = MailCredential::Password("bridge-pass".into());
        for host in ["127.0.0.1", "::1", "localhost"] {
            assert!(
                proton_smtp_transport(host, 1025, "me@example.com", &credential, TIMEOUT).is_ok(),
                "expected {host} to be accepted"
            );
        }
        let error = proton_smtp_transport(
            "smtp.example.com",
            1025,
            "me@example.com",
            &credential,
            TIMEOUT,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Proton Bridge must be reached over loopback"
        );
        assert!(proton_smtp_transport("", 1025, "me@example.com", &credential, TIMEOUT).is_err());
        assert!(
            proton_smtp_transport("127.0.0.1", 0, "me@example.com", &credential, TIMEOUT).is_err()
        );
    }

    #[test]
    fn selects_the_proton_transport_only_for_proton_accounts() {
        let credential = MailCredential::Password("bridge-pass".into());
        assert!(account_transport(&proton_account(), &credential, TIMEOUT).is_ok());

        let mut manual = account();
        manual.smtp_security = "plain".into();
        let error = account_transport(&manual, &credential, TIMEOUT).unwrap_err();
        assert_eq!(error.to_string(), "security must be tls or starttls");

        assert!(account_transport(&account(), &credential, TIMEOUT).is_ok());
    }

    #[test]
    fn proton_smtp_validation_refuses_non_loopback_before_connecting() {
        let credential = MailCredential::Password("bridge-pass".into());
        let error = validate_proton_smtp("smtp.example.com", 1025, "me@example.com", &credential)
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Proton Bridge must be reached over loopback"
        );
        assert!(validate_proton_smtp("", 1025, "me@example.com", &credential).is_err());
        assert!(validate_proton_smtp("127.0.0.1", 0, "me@example.com", &credential).is_err());
    }
}
