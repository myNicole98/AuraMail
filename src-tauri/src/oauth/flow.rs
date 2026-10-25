use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::oauth::provider::{OAuthClientConfig, OAuthGrant, OAuthProvider};

const PRODUCTION_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

fn cancelled(config: &OAuthClientConfig) -> AppError {
    AppError::Message(format!("{} sign-in was cancelled.", config.display_name()))
}

fn timed_out(config: &OAuthClientConfig) -> AppError {
    AppError::Message(format!(
        "{} sign-in timed out. Try again.",
        config.display_name()
    ))
}

fn verify_failed(config: &OAuthClientConfig) -> AppError {
    AppError::Message(format!(
        "{} sign-in could not be verified. Try again.",
        config.display_name()
    ))
}

fn browser_failed(config: &OAuthClientConfig) -> AppError {
    AppError::Message(format!(
        "Could not open the browser for {} sign-in.",
        config.display_name()
    ))
}

fn unverified_email(config: &OAuthClientConfig) -> AppError {
    AppError::Message(format!(
        "{} did not return a verified email address.",
        config.display_name()
    ))
}

fn success_html(config: &OAuthClientConfig) -> String {
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>AuraMail</title></head><body><p>{} sign-in complete. You can close this tab and return to AuraMail.</p></body></html>",
        config.display_name()
    )
}

fn error_html(config: &OAuthClientConfig) -> String {
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>AuraMail</title></head><body><p>{} sign-in did not complete. You can close this tab.</p></body></html>",
        config.display_name()
    )
}

/// Revokes a just-issued token if the flow fails after the code exchange.
/// Google supports revocation; Microsoft is a no-op because it has no
/// equivalent simple endpoint. The guard still guarantees that the active flow
/// is finished on every error path.
struct RevokeGuard {
    config: OAuthClientConfig,
    token: String,
    armed: bool,
}

impl RevokeGuard {
    fn new(config: &OAuthClientConfig, token: String) -> Self {
        Self {
            config: config.clone(),
            token,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for RevokeGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.config.revoke(&self.token);
        }
    }
}

/// Tracks the single active OAuth flow. The active flow owns an
/// `Arc<AtomicBool>` cancellation flag; `begin` installs it, `cancel` sets it,
/// and `finish` clears it only if it still belongs to the caller.
pub struct OAuthFlowState {
    active: Mutex<Option<Arc<AtomicBool>>>,
}

impl OAuthFlowState {
    pub fn new() -> Self {
        Self {
            active: Mutex::new(None),
        }
    }

    pub fn begin(&self, provider: OAuthProvider) -> AppResult<Arc<AtomicBool>> {
        let mut active = self.active.lock().map_err(|_| {
            AppError::Message(format!("{} sign-in is unavailable.", provider.display_name()))
        })?;
        if active.is_some() {
            return Err(AppError::Message(format!(
                "{} sign-in is already in progress.",
                provider.display_name()
            )));
        }
        let flag = Arc::new(AtomicBool::new(false));
        *active = Some(Arc::clone(&flag));
        Ok(flag)
    }

    pub fn cancel(&self) {
        if let Ok(active) = self.active.lock() {
            if let Some(flag) = active.as_ref() {
                flag.store(true, Ordering::SeqCst);
            }
        }
    }

    pub fn finish(&self, flag: &Arc<AtomicBool>) {
        if let Ok(mut active) = self.active.lock() {
            let owns = match active.as_ref() {
                Some(current) => Arc::ptr_eq(current, flag),
                None => false,
            };
            if owns {
                *active = None;
            }
        }
    }
}

/// Start the provider browser flow with the production five-minute timeout.
/// The frontend opens the emitted authorization URL through the same
/// `open_external` command used by mail links; this function only waits for the
/// loopback callback.
pub fn authorize(
    config: OAuthClientConfig,
    cancel: Arc<AtomicBool>,
    on_phase: &dyn Fn(&str, Option<&str>),
) -> AppResult<OAuthGrant> {
    authorize_with(
        config,
        cancel,
        PRODUCTION_TIMEOUT,
        |_| Ok(()),
        on_phase,
    )
}

/// Testable variant of `authorize`: a caller supplies the timeout and a browser
/// opener so the flow can run against a fake opener and a mock token server.
pub fn authorize_with<F, P>(
    config: OAuthClientConfig,
    cancel: Arc<AtomicBool>,
    timeout: Duration,
    opener: F,
    mut on_phase: P,
) -> AppResult<OAuthGrant>
where
    F: Fn(String) -> AppResult<()>,
    P: FnMut(&str, Option<&str>),
{
    let listener = bind_listener(&config)?;
    listener.set_nonblocking(true)?;
    let redirect_uri = config.redirect_uri(listener.local_addr()?.port());

    let state = random_token();
    let (verifier, challenge) = pkce_pair();
    let authorization_url = config.authorization_url(&redirect_uri, &state, &challenge);

    on_phase("waiting", Some(&authorization_url));
    opener(authorization_url).map_err(|_| browser_failed(&config))?;

    let code = wait_for_callback(&listener, &cancel, &state, timeout, &config)?;

    on_phase("connecting", None);

    let grant = config.exchange_code(&code, &redirect_uri, &verifier)?;

    // The grant exists from this point. If the profile, verified-email check, or
    // Microsoft's Outlook exchange fails, revoke the initial token where the
    // provider supports it before returning the error.
    let revoke_token = grant
        .refresh_token
        .clone()
        .unwrap_or_else(|| grant.access_token.clone());
    let mut cleanup = RevokeGuard::new(&config, revoke_token);

    let profile = config.fetch_profile(&grant.access_token)?;
    if !profile.email_verified {
        return Err(unverified_email(&config));
    }

    let grant = config.finalize_grant(grant, profile)?;
    cleanup.disarm();

    Ok(grant)
}

fn bind_listener(config: &OAuthClientConfig) -> std::io::Result<TcpListener> {
    TcpListener::bind(config.bind_address()).or_else(|_| TcpListener::bind("127.0.0.1:0"))
}

/// Generate a PKCE verifier (32 random bytes, base64url without padding) and its
/// SHA-256 S256 challenge.
pub fn pkce_pair() -> (String, String) {
    let mut verifier_bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut verifier_bytes);
    let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

/// Parse a loopback callback request line and return `(code, state)`, or a
/// provider-specific user-facing error for a denied consent or a mismatched
/// state.
pub fn callback_params(
    request_line: &str,
    expected_state: &str,
    config: &OAuthClientConfig,
) -> AppResult<(String, String)> {
    let target = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| verify_failed(config))?;

    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut error_description = None;

    for (key, value) in parse_query(target, config)? {
        match key.as_str() {
            "code" => code = Some(value),
            "state" => state = Some(value),
            "error" => error = Some(value),
            "error_description" => error_description = Some(value),
            _ => {}
        }
    }

    if error.is_some() {
        return Err(config.callback_error(error.as_deref(), error_description.as_deref()));
    }

    let returned_state = state.ok_or_else(|| verify_failed(config))?;
    if returned_state != expected_state {
        return Err(verify_failed(config));
    }

    let code = code.ok_or_else(|| verify_failed(config))?;
    Ok((code, returned_state))
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn parse_query(
    target: &str,
    config: &OAuthClientConfig,
) -> AppResult<Vec<(String, String)>> {
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| verify_failed(config))?;
    Ok(url
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect())
}

fn wait_for_callback(
    listener: &TcpListener,
    cancel: &AtomicBool,
    state: &str,
    timeout: Duration,
    config: &OAuthClientConfig,
) -> AppResult<String> {
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(cancelled(config));
        }
        if Instant::now() >= deadline {
            return Err(timed_out(config));
        }

        match listener.accept() {
            Ok((stream, _)) => match handle_connection(stream, state, config)? {
                Callback::Code(code) => return Ok(code),
                Callback::Ignore => {}
            },
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(_) => {
                std::thread::sleep(POLL_INTERVAL);
            }
        }
    }
}

enum Callback {
    Code(String),
    Ignore,
}

fn handle_connection(
    mut stream: TcpStream,
    state: &str,
    config: &OAuthClientConfig,
) -> AppResult<Callback> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buffer = [0u8; 4096];
    let read = match stream.read(&mut buffer) {
        Ok(size) if size > 0 => size,
        _ => return Ok(Callback::Ignore),
    };

    let request = String::from_utf8_lossy(&buffer[..read]);
    let request_line = request.lines().next().unwrap_or("");

    if request_line.contains("/favicon.ico") {
        let _ = write_response(&mut stream, "404 Not Found", "");
        return Ok(Callback::Ignore);
    }

    match callback_params(request_line, state, config) {
        Ok((code, _)) => {
            let _ = write_response(&mut stream, "200 OK", &success_html(config));
            Ok(Callback::Code(code))
        }
        Err(error) => {
            let _ = write_response(&mut stream, "200 OK", &error_html(config));
            Err(error)
        }
    }
}

fn write_response(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    let _ = stream.shutdown(Shutdown::Both);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{authorize_with, callback_params, pkce_pair, OAuthFlowState};
    use crate::error::AppResult;
    use crate::oauth::google::GoogleClientConfig;
    use crate::oauth::microsoft::MicrosoftClientConfig;
    use crate::oauth::provider::{OAuthClientConfig, OAuthProvider};

    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::time::Duration;

    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config() -> OAuthClientConfig {
        test_config_with_urls(
            "https://oauth2.googleapis.com/token",
            "https://openidconnect.googleapis.com/v1/userinfo",
        )
    }

    fn test_google_config_with_urls(
        token_url: &str,
        userinfo_url: &str,
    ) -> GoogleClientConfig {
        GoogleClientConfig {
            client_id: "client-id".into(),
            client_secret: Some("client-secret".into()),
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: token_url.to_string(),
            userinfo_url: userinfo_url.to_string(),
            revoke_url: "https://oauth2.googleapis.com/revoke".into(),
            scopes: "openid email profile https://mail.google.com/".into(),
        }
    }

    fn test_config_with_urls(token_url: &str, userinfo_url: &str) -> OAuthClientConfig {
        OAuthClientConfig::Google(test_google_config_with_urls(token_url, userinfo_url))
    }

    fn test_microsoft_config() -> OAuthClientConfig {
        test_microsoft_config_with_urls(
            "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            "https://graph.microsoft.com/v1.0/me",
        )
    }

    fn test_microsoft_config_with_urls(
        token_url: &str,
        profile_url: &str,
    ) -> OAuthClientConfig {
        OAuthClientConfig::Microsoft(MicrosoftClientConfig {
            client_id: "client-id".into(),
            auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".into(),
            token_url: token_url.to_string(),
            profile_url: profile_url.to_string(),
            scopes: vec!["openid".into()],
            outlook_scopes: vec![
                "offline_access".into(),
                "https://outlook.office.com/IMAP.AccessAsUser.All".into(),
                "https://outlook.office.com/SMTP.Send".into(),
            ],
        })
    }

    #[test]
    fn pkce_challenge_is_sha256_base64url() {
        let (verifier, challenge) = pkce_pair();
        assert!((43..=128).contains(&verifier.len()));
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        assert_eq!(challenge, expected);
    }

    #[test]
    fn callback_rejects_a_mismatched_state() {
        let request = "GET /?code=abc&state=wrong HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let error = callback_params(request, "expected", &test_config()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Google sign-in could not be verified. Try again."
        );
    }

    #[test]
    fn cancel_stops_the_waiting_flow() {
        let cancel = Arc::new(AtomicBool::new(true));
        let error = authorize_with(
            test_config(),
            cancel,
            Duration::from_millis(50),
            |_| Ok(()),
            |_, _| {},
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "Google sign-in was cancelled.");
    }

    #[test]
    fn denied_consent_is_a_cancellation() {
        let request = "GET /?error=access_denied&state=expected HTTP/1.1\r\n\r\n";
        let error = callback_params(request, "expected", &test_config()).unwrap_err();
        assert_eq!(error.to_string(), "Google sign-in was cancelled.");
    }

    #[test]
    fn microsoft_redirect_uses_localhost() {
        let config = test_microsoft_config();
        assert_eq!(config.redirect_uri(43123), "http://localhost:43123");
    }

    #[test]
    fn microsoft_admin_consent_maps_to_an_approval_message() {
        let request = "GET /?error=access_denied&error_description=AADSTS65001%3A%20consent%20required&state=expected HTTP/1.1\r\n\r\n";
        let error = callback_params(request, "expected", &test_microsoft_config()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "A Microsoft administrator must approve AuraMail for this organization before it can be connected."
        );
    }

    #[test]
    fn microsoft_user_decline_maps_to_cancellation() {
        let request = "GET /?error=access_denied&error_description=AADSTS65004%3A%20User%20declined&state=expected HTTP/1.1\r\n\r\n";
        let error = callback_params(request, "expected", &test_microsoft_config()).unwrap_err();
        assert_eq!(error.to_string(), "Microsoft sign-in was cancelled.");
    }

    #[test]
    fn timeout_returns_the_timeout_message() {
        let error = authorize_with(
            test_config(),
            Arc::new(AtomicBool::new(false)),
            Duration::from_millis(50),
            |_| Ok(()),
            |_, _| {},
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "Google sign-in timed out. Try again.");
    }

    #[test]
    fn only_one_oauth_flow_can_run() {
        let flow = OAuthFlowState::new();
        let first = flow.begin(OAuthProvider::Google).unwrap();
        assert_eq!(
            flow.begin(OAuthProvider::Google).unwrap_err().to_string(),
            "Google sign-in is already in progress."
        );
        flow.finish(&first);
        assert!(flow.begin(OAuthProvider::Microsoft).is_ok());
    }

    #[test]
    fn authorize_returns_grant_and_profile() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = runtime.block_on(async {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "access-token",
                    "refresh_token": "refresh-token",
                    "expires_in": 3600,
                })))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/userinfo"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "email": "me@gmail.com",
                    "email_verified": true,
                    "name": "Me",
                })))
                .mount(&server)
                .await;
            server
        });

        let config = test_config_with_urls(
            &format!("{}/token", server.uri()),
            &format!("{}/userinfo", server.uri()),
        );

        let opener = |url: String| -> AppResult<()> {
            let parsed = reqwest::Url::parse(&url).expect("valid auth url");
            let state = parsed
                .query_pairs()
                .find(|(key, _)| key == "state")
                .map(|(_, value)| value.into_owned())
                .expect("state present");
            let redirect_uri = parsed
                .query_pairs()
                .find(|(key, _)| key == "redirect_uri")
                .map(|(_, value)| value.into_owned())
                .expect("redirect_uri present");

            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let address = redirect_uri.trim_start_matches("http://").to_string();
                let mut stream =
                    std::net::TcpStream::connect(address).expect("connect to callback");
                let request = format!(
                    "GET /?code=good&state={state} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(request.as_bytes()).expect("write request");
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
            });
            Ok(())
        };

        let mut phases = Vec::new();
        let grant = authorize_with(
            config,
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(5),
            opener,
            |phase, url| {
                if phase == "waiting" {
                    assert!(url.is_some());
                }
                phases.push(phase.to_string());
            },
        )
        .unwrap();

        assert_eq!(
            phases,
            vec!["waiting".to_string(), "connecting".to_string()]
        );
        assert_eq!(grant.provider, OAuthProvider::Google);
        assert_eq!(grant.profile.email, "me@gmail.com");
        assert_eq!(grant.profile.username, "me@gmail.com");
        assert_eq!(grant.access_token, "access-token");
        assert_eq!(grant.refresh_token, "refresh-token");
        assert_eq!(grant.expires_in, 3600);
    }

    #[test]
    fn microsoft_authorization_bridges_to_outlook_tokens() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = runtime.block_on(async {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/token"))
                .and(body_string_contains("grant_type=authorization_code"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "graph-access",
                    "refresh_token": "graph-refresh",
                    "expires_in": 3600,
                })))
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(path("/token"))
                .and(body_string_contains("grant_type=refresh_token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "outlook-access",
                    "refresh_token": "outlook-refresh",
                    "expires_in": 1800,
                })))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/me"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "displayName": "Nicole",
                    "mail": "me@outlook.com",
                    "userPrincipalName": "me@outlook.com",
                })))
                .mount(&server)
                .await;
            server
        });

        let config = test_microsoft_config_with_urls(
            &format!("{}/token", server.uri()),
            &format!("{}/me", server.uri()),
        );

        let opener = |url: String| -> AppResult<()> {
            let parsed = reqwest::Url::parse(&url).expect("valid auth url");
            assert!(url.contains("login.microsoftonline.com"));
            let state = parsed
                .query_pairs()
                .find(|(key, _)| key == "state")
                .map(|(_, value)| value.into_owned())
                .expect("state present");
            let redirect_uri = parsed
                .query_pairs()
                .find(|(key, _)| key == "redirect_uri")
                .map(|(_, value)| value.into_owned())
                .expect("redirect_uri present");
            assert!(redirect_uri.starts_with("http://localhost:"));

            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let address = redirect_uri.trim_start_matches("http://").to_string();
                let mut stream =
                    std::net::TcpStream::connect(address).expect("connect to callback");
                let request = format!(
                    "GET /?code=good&state={state} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(request.as_bytes()).expect("write request");
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
            });
            Ok(())
        };

        let grant = authorize_with(
            config,
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(5),
            opener,
            |_, _| {},
        )
        .unwrap();

        assert_eq!(grant.provider, OAuthProvider::Microsoft);
        assert_eq!(grant.access_token, "outlook-access");
        assert_eq!(grant.refresh_token, "outlook-refresh");
        assert_eq!(grant.expires_in, 1800);
        assert_eq!(grant.profile.email, "me@outlook.com");
        assert_eq!(grant.profile.username, "me@outlook.com");
    }

    #[test]
    fn unverified_profile_revokes_the_issued_token() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server = runtime.block_on(async {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "access-token",
                    "refresh_token": "refresh-token",
                    "expires_in": 3600,
                })))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/userinfo"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "email": "me@gmail.com",
                    "email_verified": false,
                })))
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(path("/revoke"))
                .respond_with(ResponseTemplate::new(200))
                .mount(&server)
                .await;
            server
        });

        let mut google = test_google_config_with_urls(
            &format!("{}/token", server.uri()),
            &format!("{}/userinfo", server.uri()),
        );
        google.revoke_url = format!("{}/revoke", server.uri());
        let config = OAuthClientConfig::Google(google);

        let opener = |url: String| -> AppResult<()> {
            let parsed = reqwest::Url::parse(&url).expect("valid auth url");
            let state = parsed
                .query_pairs()
                .find(|(key, _)| key == "state")
                .map(|(_, value)| value.into_owned())
                .expect("state present");
            let redirect_uri = parsed
                .query_pairs()
                .find(|(key, _)| key == "redirect_uri")
                .map(|(_, value)| value.into_owned())
                .expect("redirect_uri present");

            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let address = redirect_uri.trim_start_matches("http://").to_string();
                let mut stream =
                    std::net::TcpStream::connect(address).expect("connect to callback");
                let request = format!(
                    "GET /?code=good&state={state} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(request.as_bytes()).expect("write request");
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
            });
            Ok(())
        };

        let error = authorize_with(
            config,
            Arc::new(AtomicBool::new(false)),
            Duration::from_secs(5),
            opener,
            |_, _| {},
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Google did not return a verified email address."
        );

        let requests = runtime.block_on(server.received_requests()).unwrap();
        assert!(requests
            .iter()
            .any(|request| request.method.as_str() == "POST" && request.url.path() == "/revoke"));
    }
}
