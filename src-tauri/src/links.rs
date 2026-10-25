use crate::error::{AppError, AppResult};

pub fn is_safe_external_url(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    normalized.starts_with("https://")
        || normalized.starts_with("http://")
        || normalized.starts_with("mailto:")
}

#[cfg(target_os = "linux")]
fn run_opener(program: &str, url: &str) -> bool {
    std::process::Command::new(program)
        .arg(url)
        .env_remove("BROWSER")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn open_external_url(value: &str) -> AppResult<()> {
    let url = value.trim();
    if !is_safe_external_url(url) {
        return Err(AppError::Message("unsupported external link".into()));
    }

    #[cfg(target_os = "linux")]
    {
        if run_opener("gio", url) || run_opener("xdg-open", url) {
            return Ok(());
        }
        return Err(AppError::Message(
            "could not open the link with the system browser".into(),
        ));
    }

    #[cfg(not(target_os = "linux"))]
    {
        tauri_plugin_opener::open_url(url, None::<&str>)
            .map_err(|error| AppError::Message(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::is_safe_external_url;

    #[test]
    fn accepts_web_and_mail_links() {
        assert!(is_safe_external_url("https://example.com"));
        assert!(is_safe_external_url("http://example.com"));
        assert!(is_safe_external_url("mailto:a@example.com"));
        assert!(is_safe_external_url("  HTTPS://example.com  "));
    }

    #[test]
    fn rejects_non_external_schemes() {
        assert!(!is_safe_external_url("javascript:alert(1)"));
        assert!(!is_safe_external_url("data:text/html,x"));
        assert!(!is_safe_external_url("file:///etc/passwd"));
        assert!(!is_safe_external_url("/local"));
        assert!(!is_safe_external_url(""));
    }
}
