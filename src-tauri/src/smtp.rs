use std::time::Duration;

use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{SmtpTransport, Transport};

use crate::accounts::{Account, MailSecurity};
use crate::compose::{self, ComposeInput};
use crate::error::{AppError, AppResult};
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

pub fn validate_smtp(
    host: &str,
    port: u16,
    security: MailSecurity,
    username: &str,
    credential: &MailCredential,
) -> AppResult<()> {
    let transport = transport(host, port, security, username, credential, TIMEOUT)?;

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
    let security = MailSecurity::parse(&account.smtp_security)?;
    let transport = transport(
        &account.smtp_host,
        account.smtp_port,
        security,
        &account.username,
        credential,
        SEND_TIMEOUT,
    )?;
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
}
