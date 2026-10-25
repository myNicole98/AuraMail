pub mod aura;
pub mod classification;
pub mod client;
pub mod discovery;
pub mod prompts;
pub mod rail_cache;
pub mod semantic;
pub mod voice;

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::secrets::{SecretStore, AI_KEY};
use client::AiClient;

pub const CONFIG_KEY: &str = "ai.config";
pub const ENABLED_KEY: &str = "ai.enabled";
pub const AUTO_CLASSIFY_KEY: &str = "ai.auto_classify";
pub const AUTO_RAIL_KEY: &str = "ai.auto_rail";
pub const FULL_BODY_KEY: &str = "ai.full_body_context";
pub const MATCH_VOICE_KEY: &str = "ai.match_voice";
pub(crate) const KEY_URL_KEY: &str = "ai.key_url";

fn normalized_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}

pub(crate) fn scoped_key(space_id: i64, key: &str) -> String {
    format!("space:{space_id}:{key}")
}

fn is_default_space(db: &Database, space_id: i64) -> bool {
    db.conn
        .lock()
        .unwrap()
        .query_row(
            "select lower(name) in ('default', 'personal') from spaces where id = ?1",
            rusqlite::params![space_id],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false)
}

pub(crate) fn setting_for_space(
    db: &Database,
    space_id: i64,
    key: &str,
) -> AppResult<Option<String>> {
    let scoped = db.setting(&scoped_key(space_id, key))?;
    if scoped.is_some() || !is_default_space(db, space_id) {
        return Ok(scoped);
    }
    db.setting(key)
}

fn legacy_key_name(base_url: &str) -> String {
    format!(
        "ai:{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(normalized_url(base_url))
    )
}

pub(crate) fn key_name(space_id: i64, base_url: &str) -> String {
    format!(
        "space:{space_id}:ai:{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(normalized_url(base_url))
    )
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiPreferences {
    pub auto_classify: bool,
    #[serde(default)]
    pub auto_rail: bool,
    pub full_body_context: bool,
    #[serde(default)]
    pub match_voice: bool,
}

fn bool_setting(db: &Database, space_id: i64, key: &str) -> AppResult<bool> {
    Ok(matches!(
        setting_for_space(db, space_id, key)?.as_deref(),
        Some("true") | Some("1")
    ))
}

pub fn auto_classify_enabled(db: &Database, space_id: i64) -> AppResult<bool> {
    bool_setting(db, space_id, AUTO_CLASSIFY_KEY)
}

pub fn auto_rail_enabled(db: &Database, space_id: i64) -> AppResult<bool> {
    bool_setting(db, space_id, AUTO_RAIL_KEY)
}

pub fn full_body_enabled(db: &Database, space_id: i64) -> AppResult<bool> {
    bool_setting(db, space_id, FULL_BODY_KEY)
}

pub fn match_voice_enabled(db: &Database, space_id: i64) -> AppResult<bool> {
    bool_setting(db, space_id, MATCH_VOICE_KEY)
}

pub fn preferences(db: &Database, space_id: i64) -> AppResult<AiPreferences> {
    Ok(AiPreferences {
        auto_classify: auto_classify_enabled(db, space_id)?,
        auto_rail: auto_rail_enabled(db, space_id)?,
        full_body_context: full_body_enabled(db, space_id)?,
        match_voice: match_voice_enabled(db, space_id)?,
    })
}

pub fn set_preferences(
    db: &Database,
    space_id: i64,
    preferences: &AiPreferences,
) -> AppResult<()> {
    crate::spaces::by_id(db, space_id)?;
    db.set_setting(
        &scoped_key(space_id, AUTO_CLASSIFY_KEY),
        &preferences.auto_classify.to_string(),
    )?;
    db.set_setting(
        &scoped_key(space_id, AUTO_RAIL_KEY),
        &preferences.auto_rail.to_string(),
    )?;
    db.set_setting(
        &scoped_key(space_id, FULL_BODY_KEY),
        &preferences.full_body_context.to_string(),
    )?;
    db.set_setting(
        &scoped_key(space_id, MATCH_VOICE_KEY),
        &preferences.match_voice.to_string(),
    )?;
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiConfig {
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub embedding_base_url: String,
    #[serde(default)]
    pub embedding_model: String,
    #[serde(default)]
    pub extra_headers: Vec<(String, String)>,
}

impl AiConfig {
    pub fn validate(&self) -> AppResult<()> {
        let url = reqwest::Url::parse(self.base_url.trim())
            .map_err(|_| AppError::Message("AI base URL is invalid".into()))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(AppError::Message(
                "AI base URL must start with http:// or https://".into(),
            ));
        }
        if self.model.trim().is_empty() {
            return Err(AppError::Message("AI model is required".into()));
        }
        if !self.embedding_base_url.trim().is_empty() {
            let url = reqwest::Url::parse(self.embedding_base_url.trim())
                .map_err(|_| AppError::Message("Embedding base URL is invalid".into()))?;
            if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
                return Err(AppError::Message(
                    "Embedding base URL must start with http:// or https://".into(),
                ));
            }
        }
        client::validate_extra_headers(&self.extra_headers)
    }

    pub fn load(db: &Database, space_id: i64) -> AppResult<Self> {
        match setting_for_space(db, space_id, CONFIG_KEY)? {
            Some(raw) => serde_json::from_str(&raw)
                .map_err(|_| AppError::Message("AI configuration is invalid".into())),
            None => Ok(Self::default()),
        }
    }

    pub fn save(&self, db: &Database, space_id: i64) -> AppResult<()> {
        self.validate()?;
        crate::spaces::by_id(db, space_id)?;
        let raw = serde_json::to_string(self)?;
        db.set_setting(&scoped_key(space_id, CONFIG_KEY), &raw)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiConfigInput {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub embedding_base_url: String,
    #[serde(default)]
    pub embedding_model: String,
    #[serde(default)]
    pub extra_headers: Vec<(String, String)>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub embedding_api_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AiConfigStatus {
    pub base_url: String,
    pub model: String,
    pub embedding_base_url: String,
    pub embedding_model: String,
    pub extra_headers: Vec<(String, String)>,
    pub enabled: bool,
    pub configured: bool,
    pub has_key: bool,
    pub has_embedding_key: bool,
}

pub(crate) fn stored_key_for(
    db: &Database,
    secrets: &dyn SecretStore,
    space_id: i64,
    base_url: &str,
) -> Option<String> {
    let key = key_name(space_id, base_url);
    if let Ok(value) = secrets.get(&key) {
        return Some(value);
    }
    if !is_default_space(db, space_id) {
        return None;
    }
    let legacy = legacy_key_name(base_url);
    if let Ok(value) = secrets.get(&legacy) {
        let _ = secrets.set(&key, &value);
        return Some(value);
    }
    let owner = db
        .setting(&scoped_key(space_id, KEY_URL_KEY))
        .ok()
        .flatten()
        .or_else(|| db.setting(KEY_URL_KEY).ok().flatten())?;
    if normalized_url(&owner) != normalized_url(base_url) {
        return None;
    }
    let value = secrets.get(AI_KEY).ok()?;
    let _ = secrets.set(&key, &value);
    Some(value)
}

fn configured(config: &AiConfig) -> bool {
    !config.base_url.trim().is_empty() && !config.model.trim().is_empty()
}

pub fn enabled(db: &Database, space_id: i64) -> AppResult<bool> {
    if let Some(value) = setting_for_space(db, space_id, ENABLED_KEY)? {
        return Ok(matches!(value.as_str(), "true" | "1"));
    }
    // Spaces configured before the explicit toggle existed stay enabled.
    Ok(configured(&AiConfig::load(db, space_id)?))
}

pub fn set_enabled(db: &Database, space_id: i64, enabled: bool) -> AppResult<()> {
    crate::spaces::by_id(db, space_id)?;
    db.set_setting(
        &scoped_key(space_id, ENABLED_KEY),
        &enabled.to_string(),
    )
}

pub fn status(
    db: &Database,
    secrets: &dyn SecretStore,
    space_id: i64,
) -> AppResult<AiConfigStatus> {
    let config = AiConfig::load(db, space_id)?;
    Ok(AiConfigStatus {
        enabled: enabled(db, space_id)?,
        configured: configured(&config),
        has_key: stored_key_for(db, secrets, space_id, &config.base_url).is_some(),
        has_embedding_key: !config.embedding_base_url.trim().is_empty()
            && stored_key_for(db, secrets, space_id, &config.embedding_base_url).is_some(),
        base_url: config.base_url,
        model: config.model,
        embedding_base_url: config.embedding_base_url,
        embedding_model: config.embedding_model,
        extra_headers: config.extra_headers,
    })
}

pub fn space_enabled(db: &Database, space_id: i64) -> AppResult<bool> {
    if !enabled(db, space_id)? {
        return Ok(false);
    }
    Ok(configured(&AiConfig::load(db, space_id)?))
}

pub fn configured_client(
    db: &Database,
    secrets: &dyn SecretStore,
    space_id: i64,
) -> AppResult<(AiConfig, AiClient)> {
    let config = AiConfig::load(db, space_id)?;
    if !space_enabled(db, space_id)? {
        return Err(AppError::Message("AI is disabled for this space".into()));
    }
    let api_key =
        stored_key_for(db, secrets, space_id, &config.base_url).unwrap_or_default();
    let client = AiClient::try_new(config.clone(), &api_key)?;
    Ok((config, client))
}

pub fn configured_embedding_client(
    db: &Database,
    secrets: &dyn SecretStore,
    space_id: i64,
) -> AppResult<(String, AiClient)> {
    let config = AiConfig::load(db, space_id)?;
    if !space_enabled(db, space_id)? {
        return Err(AppError::Message("AI is disabled for this space".into()));
    }
    if config.embedding_base_url.trim().is_empty()
        || config.embedding_model.trim().is_empty()
    {
        return Err(AppError::Message(
            "Embedding model is not configured".into(),
        ));
    }
    let api_key =
        stored_key_for(db, secrets, space_id, &config.embedding_base_url)
            .unwrap_or_default();
    let client = AiClient::try_new(
        AiConfig {
            base_url: config.embedding_base_url,
            model: config.embedding_model.clone(),
            embedding_base_url: String::new(),
            embedding_model: String::new(),
            extra_headers: config.extra_headers,
        },
        &api_key,
    )?;
    Ok((config.embedding_model, client))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailContext {
    pub from_name: String,
    pub from_addr: String,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AiDraft {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAuraDraft {
    pub tone: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAuraAskResponse {
    pub answer: String,
    pub drafts: Vec<AiAuraDraft>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAuraRail {
    pub summary: String,
    pub ask: String,
    pub deadline: Option<String>,
    pub tone: Option<String>,
    pub question: Option<String>,
    pub options: Vec<String>,
    pub drafts: Vec<AiAuraDraft>,
    pub message_count: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatRequest {
    pub context: String,
    #[serde(default)]
    pub message_id: Option<i64>,
    pub question: String,
    #[serde(default)]
    pub history: Vec<client::ChatMessage>,
    #[serde(default)]
    pub full_body: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AiChatResponse {
    pub answer: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiComposeRequest {
    pub mode: String,
    #[serde(default)]
    pub instruction: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AiComposeResponse {
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::{AiConfig, CONFIG_KEY};
    use crate::db::Database;
    use crate::secrets::{MemoryStore, SecretStore, AI_KEY};

    fn default_space(db: &Database) -> i64 {
        crate::spaces::default_id(db).unwrap()
    }

    fn config() -> AiConfig {
        AiConfig {
            base_url: "https://api.example.com/v1".into(),
            model: "gpt-test".into(),
            embedding_base_url: "https://embed.example.com/v1".into(),
            embedding_model: "embed-test".into(),
            extra_headers: vec![("x-org".into(), "auramail".into())],
        }
    }

    #[test]
    fn ai_configuration_is_isolated_per_space() {
        let db = Database::open_memory().unwrap();
        let default = crate::spaces::ensure_default(&db).unwrap();
        let work = crate::spaces::create(&db, "Work").unwrap();
        AiConfig {
            base_url: "https://ai.example/v1".into(),
            model: "work-model".into(),
            ..AiConfig::default()
        }
        .save(&db, work.id)
        .unwrap();
        assert_eq!(AiConfig::load(&db, work.id).unwrap().model, "work-model");
        assert_eq!(AiConfig::load(&db, default.id).unwrap(), AiConfig::default());
        assert!(!super::auto_classify_enabled(&db, work.id).unwrap());
    }

    #[test]
    fn ai_keys_are_isolated_per_space() {
        let db = Database::open_memory().unwrap();
        let default = crate::spaces::ensure_default(&db).unwrap();
        let work = crate::spaces::create(&db, "Work").unwrap();
        let secrets = MemoryStore::default();
        let url = "https://ai.example/v1";
        secrets
            .set(&super::key_name(work.id, url), "work-secret")
            .unwrap();
        assert_eq!(
            super::stored_key_for(&db, &secrets, work.id, url).as_deref(),
            Some("work-secret")
        );
        assert!(super::stored_key_for(&db, &secrets, default.id, url).is_none());
    }

    #[test]
    fn config_round_trips_in_settings_without_the_api_key() {
        let db = Database::open_memory().unwrap();
        config().save(&db, default_space(&db)).unwrap();
        assert_eq!(AiConfig::load(&db, default_space(&db)).unwrap(), config());
        let raw = db
            .setting(&super::scoped_key(default_space(&db), CONFIG_KEY))
            .unwrap()
            .unwrap();
        assert!(!raw.contains("secret-key"));
        assert!(SecretStore::get(&MemoryStore::default(), AI_KEY).is_err());
    }

    #[test]
    fn validation_requires_http_scheme_model_and_safe_headers() {
        let mut invalid = config();
        invalid.base_url = "api.example.com".into();
        assert!(invalid.validate().is_err());

        let mut missing_model = config();
        missing_model.model = "  ".into();
        assert!(missing_model.validate().is_err());

        let mut bad_header = config();
        bad_header.extra_headers = vec![("x-org".into(), "line\nbreak".into())];
        assert!(bad_header.validate().is_err());

        assert!(config().validate().is_ok());
    }

    #[test]
    fn defaults_to_an_unconfigured_empty_config() {
        let db = Database::open_memory().unwrap();
        let config = AiConfig::load(&db, default_space(&db)).unwrap();
        assert_eq!(config.base_url, "");
        assert_eq!(config.model, "");
        assert!(config.extra_headers.is_empty());
    }

    #[test]
    fn status_is_configured_without_an_api_key_for_keyless_providers() {
        let db = Database::open_memory().unwrap();
        config().save(&db, default_space(&db)).unwrap();
        let secrets = MemoryStore::default();

        let status = super::status(&db, &secrets, default_space(&db)).unwrap();
        assert!(status.enabled);
        assert!(status.configured);
        assert!(!status.has_key);
        assert!(super::configured_client(&db, &secrets, default_space(&db)).is_ok());

        secrets
            .set(&super::key_name(default_space(&db), &config().base_url), "secret")
            .unwrap();
        assert!(super::status(&db, &secrets, default_space(&db)).unwrap().has_key);
    }

    #[test]
    fn stored_keys_belong_to_their_base_url() {
        let db = Database::open_memory().unwrap();
        config().save(&db, default_space(&db)).unwrap();
        let secrets = MemoryStore::default();
        secrets
            .set(&super::key_name(default_space(&db), &config().base_url), "secret")
            .unwrap();
        assert!(super::status(&db, &secrets, default_space(&db)).unwrap().has_key);
        assert!(super::stored_key_for(&db, &secrets, default_space(&db), &config().base_url).is_some());

        let mut other = config();
        other.base_url = "https://other.example.com/v1".into();
        other.save(&db, default_space(&db)).unwrap();
        assert!(!super::status(&db, &secrets, default_space(&db)).unwrap().has_key);
        assert!(super::stored_key_for(&db, &secrets, default_space(&db), &config().base_url).is_some());
        assert!(super::stored_key_for(&db, &secrets, default_space(&db), &other.base_url).is_none());

        secrets
            .set(&super::key_name(default_space(&db), &other.base_url), "other-secret")
            .unwrap();
        assert!(super::stored_key_for(&db, &secrets, default_space(&db), &other.base_url).is_some());
    }

    #[test]
    fn migrates_the_legacy_single_key_to_the_matching_base_url() {
        let db = Database::open_memory().unwrap();
        config().save(&db, default_space(&db)).unwrap();
        let secrets = MemoryStore::default();
        secrets.set(AI_KEY, "legacy-secret").unwrap();
        db.set_setting(super::KEY_URL_KEY, &config().base_url)
            .unwrap();

        assert_eq!(
            super::stored_key_for(&db, &secrets, default_space(&db), &config().base_url).as_deref(),
            Some("legacy-secret")
        );
        assert_eq!(
            secrets
                .get(&super::key_name(default_space(&db), &config().base_url))
                .unwrap(),
            "legacy-secret"
        );
    }

    #[test]
    fn ai_can_be_disabled_without_losing_its_configuration() {
        let db = Database::open_memory().unwrap();
        let space = default_space(&db);
        config().save(&db, space).unwrap();

        // Legacy configured spaces are enabled without an explicit flag.
        assert!(super::enabled(&db, space).unwrap());
        assert!(super::space_enabled(&db, space).unwrap());

        super::set_enabled(&db, space, false).unwrap();
        assert!(!super::enabled(&db, space).unwrap());
        assert!(!super::space_enabled(&db, space).unwrap());
        assert_eq!(AiConfig::load(&db, space).unwrap(), config());

        super::set_enabled(&db, space, true).unwrap();
        assert!(super::enabled(&db, space).unwrap());
        assert!(super::space_enabled(&db, space).unwrap());
    }

    #[test]
    fn enabling_an_unconfigured_space_reveals_setup_but_keeps_ai_unusable() {
        let db = Database::open_memory().unwrap();
        let space = crate::spaces::create(&db, "Work").unwrap();

        assert!(!super::enabled(&db, space.id).unwrap());
        super::set_enabled(&db, space.id, true).unwrap();
        assert!(super::enabled(&db, space.id).unwrap());
        assert!(!super::space_enabled(&db, space.id).unwrap());
    }

    #[test]
    fn ai_preferences_default_off_and_persist() {
        let db = Database::open_memory().unwrap();
        assert!(!super::auto_classify_enabled(&db, default_space(&db)).unwrap());
        assert!(!super::auto_rail_enabled(&db, default_space(&db)).unwrap());
        assert!(!super::full_body_enabled(&db, default_space(&db)).unwrap());
        assert!(!super::match_voice_enabled(&db, default_space(&db)).unwrap());
        super::set_preferences(
            &db,
            default_space(&db),
            &super::AiPreferences {
                auto_classify: true,
                auto_rail: true,
                full_body_context: true,
                match_voice: true,
            },
        )
        .unwrap();
        assert!(super::auto_classify_enabled(&db, default_space(&db)).unwrap());
        assert!(super::auto_rail_enabled(&db, default_space(&db)).unwrap());
        assert!(super::full_body_enabled(&db, default_space(&db)).unwrap());
        assert!(super::match_voice_enabled(&db, default_space(&db)).unwrap());
        assert!(super::preferences(&db, default_space(&db)).unwrap().auto_rail);
    }
}
