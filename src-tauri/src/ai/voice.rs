use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::error::{AppError, AppResult};

use super::match_voice_enabled;

pub const VOICE_PROFILE_KEY: &str = "ai.voice_profile";
pub const VOICE_LAST_ATTEMPT_KEY: &str = "ai.voice_last_attempt";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiVoiceProfile {
    pub text: String,
    pub updated_at: i64,
    pub sample_count: u32,
}

pub fn get_voice_profile(
    db: &Database,
    space_id: i64,
) -> AppResult<Option<AiVoiceProfile>> {
    let Some(raw) = super::setting_for_space(db, space_id, VOICE_PROFILE_KEY)? else {
        return Ok(None);
    };
    Ok(serde_json::from_str(&raw).ok())
}

pub fn set_voice_profile(
    db: &Database,
    space_id: i64,
    profile: &AiVoiceProfile,
) -> AppResult<()> {
    crate::spaces::by_id(db, space_id)?;
    let raw = serde_json::to_string(profile)
        .map_err(|_| AppError::Message("Could not save the voice profile".into()))?;
    db.set_setting(&super::scoped_key(space_id, VOICE_PROFILE_KEY), &raw)
}

pub fn clear_voice_profile(db: &Database, space_id: i64) -> AppResult<()> {
    db.delete_setting(&super::scoped_key(space_id, VOICE_PROFILE_KEY))
}

pub fn voice_prompt_text(db: &Database, space_id: i64) -> AppResult<Option<String>> {
    if !match_voice_enabled(db, space_id)? {
        return Ok(None);
    }
    let text = get_voice_profile(db, space_id)?
        .map(|profile| profile.text.trim().to_string())
        .filter(|text| !text.is_empty());
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{AiPreferences, set_preferences};
    use crate::db::Database;

    fn default_space(db: &Database) -> i64 {
        crate::spaces::default_id(db).unwrap()
    }

    fn profile() -> AiVoiceProfile {
        AiVoiceProfile {
            text: "- Warm, direct greetings\n- Short sentences".into(),
            updated_at: 123,
            sample_count: 4,
        }
    }

    #[test]
    fn stores_and_reads_a_voice_profile() {
        let db = Database::open_memory().unwrap();
        set_voice_profile(&db, default_space(&db), &profile()).unwrap();

        assert_eq!(get_voice_profile(&db, default_space(&db)).unwrap(), Some(profile()));
    }

    #[test]
    fn clears_a_voice_profile() {
        let db = Database::open_memory().unwrap();
        set_voice_profile(&db, default_space(&db), &profile()).unwrap();
        clear_voice_profile(&db, default_space(&db)).unwrap();

        assert_eq!(get_voice_profile(&db, default_space(&db)).unwrap(), None);
    }

    #[test]
    fn only_supplies_the_profile_when_voice_matching_is_on() {
        let db = Database::open_memory().unwrap();
        set_voice_profile(&db, default_space(&db), &profile()).unwrap();
        set_preferences(
            &db,
            default_space(&db),
            &AiPreferences {
                match_voice: false,
                ..AiPreferences::default()
            },
        )
        .unwrap();

        assert_eq!(voice_prompt_text(&db, default_space(&db)).unwrap(), None);

        set_preferences(
            &db,
            default_space(&db),
            &AiPreferences {
                match_voice: true,
                ..AiPreferences::default()
            },
        )
        .unwrap();

        assert_eq!(
            voice_prompt_text(&db, default_space(&db)).unwrap().as_deref(),
            Some("- Warm, direct greetings\n- Short sentences")
        );
    }
}
