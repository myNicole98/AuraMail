use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize)]
struct GoogleOAuthFile {
    client_id: String,
    #[serde(default)]
    client_secret: String,
}

#[derive(serde::Deserialize)]
struct MicrosoftOAuthFile {
    client_id: String,
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn resolve_from_file(path: &Path) -> (Option<String>, Option<String>) {
    if !path.exists() {
        return (None, None);
    }
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
    let parsed: GoogleOAuthFile = serde_json::from_str(&raw).unwrap_or_else(|error| {
        panic!(
            "invalid {}: expected {{\"client_id\":\"...\",\"client_secret\":\"...\"}}: {error}",
            path.display()
        )
    });
    (non_empty(Some(parsed.client_id)), non_empty(Some(parsed.client_secret)))
}

fn rust_option(value: Option<String>) -> String {
    match value {
        Some(value) => format!("Some({value:?})"),
        None => "None".to_string(),
    }
}

fn resolve_microsoft_from_file(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
    let parsed: MicrosoftOAuthFile = serde_json::from_str(&raw).unwrap_or_else(|error| {
        panic!(
            "invalid {}: expected {{\"client_id\":\"...\"}}: {error}",
            path.display()
        )
    });
    non_empty(Some(parsed.client_id))
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let config_path = manifest_dir.join("google-oauth.json");
    println!("cargo:rerun-if-changed={}", config_path.display());
    println!("cargo:rerun-if-env-changed=AURAMAIL_GOOGLE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=AURAMAIL_GOOGLE_CLIENT_SECRET");

    let env_id = non_empty(env::var("AURAMAIL_GOOGLE_CLIENT_ID").ok());
    let env_secret = non_empty(env::var("AURAMAIL_GOOGLE_CLIENT_SECRET").ok());
    let (client_id, client_secret) = match env_id {
        Some(client_id) => (Some(client_id), env_secret),
        None => resolve_from_file(&config_path),
    };

    let microsoft_config_path = manifest_dir.join("microsoft-oauth.json");
    println!("cargo:rerun-if-changed={}", microsoft_config_path.display());
    println!("cargo:rerun-if-env-changed=AURAMAIL_MICROSOFT_CLIENT_ID");
    let microsoft_client_id = non_empty(env::var("AURAMAIL_MICROSOFT_CLIENT_ID").ok())
        .or_else(|| resolve_microsoft_from_file(&microsoft_config_path));

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));
    let generated = format!(
        "pub const EMBEDDED_GOOGLE_CLIENT_ID: Option<&str> = {};\n\
         pub const EMBEDDED_GOOGLE_CLIENT_SECRET: Option<&str> = {};\n",
        rust_option(client_id),
        rust_option(client_secret)
    );
    fs::write(out_dir.join("google_oauth_config.rs"), generated)
        .expect("write generated Google OAuth config");

    let microsoft_generated = format!(
        "pub const EMBEDDED_MICROSOFT_CLIENT_ID: Option<&str> = {};\n",
        rust_option(microsoft_client_id)
    );
    fs::write(
        out_dir.join("microsoft_oauth_config.rs"),
        microsoft_generated,
    )
    .expect("write generated Microsoft OAuth config");

    tauri_build::build()
}
