use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use notify::{Event, EventKind, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::spaces;
use crate::AppState;

pub const CHOICE_KEY: &str = "theme.choice";
pub const ACCENT_KEY: &str = "theme.accent";
pub const BACKGROUND_KEY: &str = "theme.background";
pub const PANES_KEY: &str = "theme.panes";
pub const STYLE_KEY: &str = "theme.style";
pub const MODE_KEY: &str = "theme.mode";
pub const LIGHT_PRESET_KEY: &str = "theme.light_preset";
pub const DARK_PRESET_KEY: &str = "theme.dark_preset";
pub const MATUGEN_KEY: &str = "theme.matugen";
pub const DEFAULT_CHOICE: &str = "system";
pub const DEFAULT_BACKGROUND: &str = "solid";
pub const DEFAULT_PANES: &str = "straight";
const TEMPLATE_FILE: &str = "theme_template.json";
const BUILT_IN_TEMPLATE: &str = include_str!("../../resources/matugen/auramail.json");

struct WatchHandle {
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
}

static WATCHER: Mutex<Option<WatchHandle>> = Mutex::new(None);
static ACTIVE_SPACE: AtomicI64 = AtomicI64::new(0);

fn scoped_key(space_id: i64, key: &str) -> String {
    format!("space:{space_id}:{key}")
}

fn is_default_space(db: &Database, space_id: i64) -> AppResult<bool> {
    Ok(db
        .conn
        .lock()
        .unwrap()
        .query_row(
            "select lower(name) in ('default', 'personal') from spaces where id = ?1",
            rusqlite::params![space_id],
            |row| row.get(0),
        )
        .unwrap_or(false))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Base16Palette {
    pub scheme: String,
    #[serde(rename = "base00")]
    pub base00: String,
    #[serde(rename = "base01")]
    pub base01: String,
    #[serde(rename = "base02")]
    pub base02: String,
    #[serde(rename = "base03")]
    pub base03: String,
    #[serde(rename = "base04")]
    pub base04: String,
    #[serde(rename = "base05")]
    pub base05: String,
    #[serde(rename = "base06")]
    pub base06: String,
    #[serde(rename = "base07")]
    pub base07: String,
    #[serde(rename = "base08")]
    pub base08: String,
    #[serde(rename = "base09")]
    pub base09: String,
    #[serde(rename = "base0A")]
    pub base0a: String,
    #[serde(rename = "base0B")]
    pub base0b: String,
    #[serde(rename = "base0C")]
    pub base0c: String,
    #[serde(rename = "base0D")]
    pub base0d: String,
    #[serde(rename = "base0E")]
    pub base0e: String,
    #[serde(rename = "base0F")]
    pub base0f: String,
}

impl Base16Palette {
    fn tokens(&self) -> [&str; 16] {
        [
            &self.base00,
            &self.base01,
            &self.base02,
            &self.base03,
            &self.base04,
            &self.base05,
            &self.base06,
            &self.base07,
            &self.base08,
            &self.base09,
            &self.base0a,
            &self.base0b,
            &self.base0c,
            &self.base0d,
            &self.base0e,
            &self.base0f,
        ]
    }
}

fn default_background() -> String {
    DEFAULT_BACKGROUND.into()
}

fn default_panes() -> String {
    DEFAULT_PANES.into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeSettings {
    pub choice: String,
    pub accent: Option<String>,
    #[serde(default = "default_background")]
    pub background: String,
    #[serde(default = "default_panes")]
    pub panes: String,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            choice: DEFAULT_CHOICE.into(),
            accent: None,
            background: DEFAULT_BACKGROUND.into(),
            panes: DEFAULT_PANES.into(),
        }
    }
}

fn is_hex_color(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(u8::is_ascii_hexdigit)
}

fn validate_palette(palette: &Base16Palette) -> AppResult<()> {
    if palette.scheme.trim().is_empty() || !palette.tokens().iter().all(|token| is_hex_color(token))
    {
        return Err(AppError::Message(
            "theme requires a scheme and sixteen six-digit #rrggbb tokens".into(),
        ));
    }
    Ok(())
}

pub fn parse_palette(raw: &str) -> AppResult<Base16Palette> {
    let palette: Base16Palette = serde_json::from_str(raw)
        .map_err(|_| AppError::Message("theme file is not a valid Base16 palette".into()))?;
    validate_palette(&palette)?;
    Ok(palette)
}

pub fn read_palette(path: &Path) -> AppResult<Base16Palette> {
    let raw = std::fs::read_to_string(path)
        .map_err(|_| AppError::Message("theme file could not be read".into()))?;
    parse_palette(&raw)
}

fn theme_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .config_dir()
        .map_err(|_| AppError::Message("config directory is unavailable".into()))?;
    Ok(dir.join("auramail"))
}

pub fn theme_path(app: &AppHandle) -> AppResult<PathBuf> {
    Ok(theme_dir(app)?.join("theme.json"))
}

pub fn ensure_config(app: &AppHandle) -> AppResult<()> {
    ensure_config_at(&theme_dir(app)?)
}

fn ensure_config_at(dir: &Path) -> AppResult<()> {
    std::fs::create_dir_all(dir)?;
    let template = dir.join(TEMPLATE_FILE);
    if !template.exists() {
        std::fs::write(template, BUILT_IN_TEMPLATE)?;
    }
    Ok(())
}

fn valid_background(background: &str) -> bool {
    matches!(background, "solid" | "glass")
}

fn valid_panes(panes: &str) -> bool {
    matches!(panes, "straight" | "rounded")
}

fn legacy_theme_shape(style: Option<String>) -> (String, String) {
    match style.as_deref() {
        Some("frosted") => ("glass".into(), "straight".into()),
        Some("fancy") => ("glass".into(), "rounded".into()),
        _ => ("solid".into(), "straight".into()),
    }
}

fn valid_choice(choice: &str) -> bool {
    matches!(
        choice,
        "system"
            | "light"
            | "dark"
            | "matugen"
            | "catppuccin"
            | "nord"
            | "gruvbox"
            | "tokyo-night"
            | "dracula"
            | "one-dark"
            | "solarized"
            | "kanagawa"
            | "rose-pine"
            | "everforest"
    )
}

fn preset_choice(value: Option<String>) -> Option<&'static str> {
    let preset = value?.to_lowercase();
    if preset.starts_with("catppuccin") {
        Some("catppuccin")
    } else if preset == "nord" {
        Some("nord")
    } else if preset.starts_with("gruvbox") {
        Some("gruvbox")
    } else if preset == "tokyo-night" {
        Some("tokyo-night")
    } else if preset == "dracula" {
        Some("dracula")
    } else if preset.starts_with("rose-pine") {
        Some("rose-pine")
    } else {
        None
    }
}

fn legacy_matugen_enabled(db: &Database) -> AppResult<bool> {
    Ok(matches!(
        db.setting(MATUGEN_KEY)?.as_deref(),
        Some("true") | Some("1")
    ))
}

fn legacy_choice(db: &Database) -> AppResult<String> {
    if legacy_matugen_enabled(db)? {
        return Ok("matugen".into());
    }
    let dark_choice = preset_choice(db.setting(DARK_PRESET_KEY)?);
    let light_choice = preset_choice(db.setting(LIGHT_PRESET_KEY)?);
    if let Some(choice) = dark_choice.or(light_choice) {
        return Ok(choice.into());
    }
    Ok(match db.setting(MODE_KEY)?.as_deref() {
        Some("light") => "light".into(),
        Some("dark") => "dark".into(),
        _ => DEFAULT_CHOICE.into(),
    })
}

pub fn load_settings(db: &Database, space_id: i64) -> AppResult<ThemeSettings> {
    let default_space = is_default_space(db, space_id)?;
    let choice = match db.setting(&scoped_key(space_id, CHOICE_KEY))? {
        Some(choice) if valid_choice(&choice) => choice,
        Some(_) => DEFAULT_CHOICE.into(),
        None if default_space => legacy_choice(db)?,
        None => DEFAULT_CHOICE.into(),
    };
    let accent = db
        .setting(&scoped_key(space_id, ACCENT_KEY))?
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            default_space
                .then(|| db.setting(ACCENT_KEY).ok().flatten())
                .flatten()
        })
        .filter(|value| !value.trim().is_empty());
    let (legacy_background, legacy_panes) = legacy_theme_shape(db.setting(STYLE_KEY)?);
    let background = match db.setting(&scoped_key(space_id, BACKGROUND_KEY))? {
        Some(background) if valid_background(&background) => background,
        Some(_) => DEFAULT_BACKGROUND.into(),
        None if default_space => legacy_background,
        None => DEFAULT_BACKGROUND.into(),
    };
    let panes = match db.setting(&scoped_key(space_id, PANES_KEY))? {
        Some(panes) if valid_panes(&panes) => panes,
        Some(_) => DEFAULT_PANES.into(),
        None if default_space => legacy_panes,
        None => DEFAULT_PANES.into(),
    };
    Ok(ThemeSettings {
        choice,
        accent,
        background,
        panes,
    })
}

pub fn save_settings(
    db: &Database,
    space_id: i64,
    settings: &ThemeSettings,
) -> AppResult<()> {
    spaces::by_id(db, space_id)?;
    if !valid_choice(settings.choice.trim()) {
        return Err(AppError::Message("unknown theme choice".into()));
    }
    if !valid_background(settings.background.trim()) {
        return Err(AppError::Message("unknown theme background".into()));
    }
    if !valid_panes(settings.panes.trim()) {
        return Err(AppError::Message("unknown theme panes".into()));
    }
    db.set_setting(
        &scoped_key(space_id, CHOICE_KEY),
        settings.choice.trim(),
    )?;
    db.set_setting(
        &scoped_key(space_id, ACCENT_KEY),
        settings
            .accent
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(""),
    )?;
    db.set_setting(
        &scoped_key(space_id, BACKGROUND_KEY),
        settings.background.trim(),
    )?;
    db.set_setting(&scoped_key(space_id, PANES_KEY), settings.panes.trim())?;
    Ok(())
}

pub fn matugen_enabled(db: &Database, space_id: i64) -> AppResult<bool> {
    Ok(load_settings(db, space_id)?.choice == "matugen")
}

pub fn set_active_theme_space(
    db: &Database,
    app: &AppHandle,
    space_id: i64,
) -> AppResult<()> {
    spaces::by_id(db, space_id)?;
    ACTIVE_SPACE.store(space_id, Ordering::SeqCst);
    if matugen_enabled(db, space_id)? {
        watch(app.clone(), theme_path(app)?)?;
    } else {
        unwatch();
    }
    Ok(())
}

pub fn watch(app: AppHandle, path: PathBuf) -> AppResult<()> {
    let mut slot = WATCHER
        .lock()
        .map_err(|_| AppError::Message("theme watcher is unavailable".into()))?;
    if slot.is_some() {
        return Ok(());
    }
    let parent = path
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| AppError::Message("theme path is invalid".into()))?;
    std::fs::create_dir_all(&parent)?;
    let (tx, rx) = mpsc::channel::<()>();
    let target = path.clone();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        if let Ok(event) = result {
            let relevant = matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
            );
            if relevant && event.paths.iter().any(|candidate| candidate == &target) {
                let _ = tx.send(());
            }
        }
    })
    .map_err(|_| AppError::Message("theme file could not be watched".into()))?;
    watcher
        .watch(&parent, RecursiveMode::NonRecursive)
        .map_err(|_| AppError::Message("theme file could not be watched".into()))?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let thread = std::thread::spawn(move || {
        let _watcher = watcher;
        loop {
            if stop_thread.load(Ordering::SeqCst) {
                break;
            }
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(()) => {
                    while rx.recv_timeout(Duration::from_millis(150)).is_ok() {}
                    if stop_thread.load(Ordering::SeqCst) {
                        break;
                    }
                    let active_space = ACTIVE_SPACE.load(Ordering::SeqCst);
                    let enabled = active_space > 0
                        && app
                            .try_state::<AppState>()
                            .map(|state| {
                                matugen_enabled(&state.db, active_space).unwrap_or(false)
                            })
                            .unwrap_or(false);
                    if !enabled {
                        continue;
                    }
                    match read_palette(&path) {
                        Ok(palette) => {
                            let _ = app.emit("theme://changed", &palette);
                        }
                        Err(error) => {
                            if path.exists() {
                                let _ = app.emit("theme://error", error.to_string());
                            }
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
    *slot = Some(WatchHandle { stop, thread });
    Ok(())
}

pub fn unwatch() {
    let handle = match WATCHER.lock() {
        Ok(mut slot) => slot.take(),
        Err(_) => None,
    };
    if let Some(handle) = handle {
        handle.stop.store(true, Ordering::SeqCst);
        let _ = handle.thread.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_space(db: &Database) -> i64 {
        crate::spaces::default_id(db).unwrap()
    }

    const VALID: &str = r##"{"scheme":"Test","base00":"#101010","base01":"#202020","base02":"#303030","base03":"#404040","base04":"#505050","base05":"#e0e0e0","base06":"#f0f0f0","base07":"#ffffff","base08":"#ff0000","base09":"#ff8800","base0A":"#ffff00","base0B":"#00ff00","base0C":"#00ffff","base0D":"#0000ff","base0E":"#ff00ff","base0F":"#884400"}"##;

    #[test]
    fn parses_flat_base16_json() {
        let palette = parse_palette(VALID).unwrap();
        assert_eq!(palette.scheme, "Test");
        assert_eq!(palette.base00, "#101010");
        assert_eq!(palette.base0a, "#ffff00");
        assert_eq!(palette.base0f, "#884400");
    }

    #[test]
    fn rejects_short_hex_values() {
        let raw = VALID.replace("#101010", "#101");
        assert!(parse_palette(&raw).is_err());
    }

    #[test]
    fn rejects_missing_or_malformed_tokens() {
        let raw = VALID.replace("\"base0F\":\"#884400\"", "\"base0F\":\"884400\"");
        assert!(parse_palette(&raw).is_err());
        let raw = VALID.replace("\"base0A\":\"#ffff00\",", "");
        assert!(parse_palette(&raw).is_err());
        assert!(parse_palette("not json").is_err());
    }

    #[test]
    fn serializes_base16_keys_canonically() {
        let palette = parse_palette(VALID).unwrap();
        let value = serde_json::to_value(&palette).unwrap();
        assert_eq!(value["base0A"], "#ffff00");
        assert_eq!(value["base0F"], "#884400");
        assert_eq!(value["scheme"], "Test");
    }

    #[test]
    fn reads_palettes_and_reports_safe_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.json");
        std::fs::write(&path, VALID).unwrap();
        assert_eq!(read_palette(&path).unwrap().scheme, "Test");

        std::fs::write(&path, VALID.replace("#101010", "#101")).unwrap();
        let message = read_palette(&path).unwrap_err().to_string();
        assert!(!message.contains("#101"));

        assert!(read_palette(&dir.path().join("missing.json")).is_err());
    }

    #[test]
    fn theme_settings_are_isolated_per_space() {
        let db = Database::open_memory().unwrap();
        let default = crate::spaces::ensure_default(&db).unwrap();
        let work = crate::spaces::create(&db, "Work").unwrap();
        save_settings(
            &db,
            work.id,
            &ThemeSettings {
                choice: "dark".into(),
                accent: Some("#ff0000".into()),
                background: "glass".into(),
                panes: "rounded".into(),
            },
        )
        .unwrap();
        let work_settings = load_settings(&db, work.id).unwrap();
        let default_settings = load_settings(&db, default.id).unwrap();
        assert_eq!(work_settings.choice, "dark");
        assert_eq!(work_settings.background, "glass");
        assert_eq!(default_settings, ThemeSettings::default());
        assert!(!matugen_enabled(&db, work.id).unwrap());
    }

    #[test]
    fn migrates_legacy_theme_keys_to_the_default_space() {
        let db = Database::open_memory().unwrap();
        let default = crate::spaces::ensure_default(&db).unwrap();
        let work = crate::spaces::create(&db, "Work").unwrap();
        db.set_setting(CHOICE_KEY, "dark").unwrap();
        db.set_setting(BACKGROUND_KEY, "glass").unwrap();
        db.set_setting(PANES_KEY, "rounded").unwrap();
        db.conn
            .lock()
            .unwrap()
            .pragma_update(None, "user_version", 21)
            .unwrap();
        db.migrate().unwrap();

        let migrated = load_settings(&db, default.id).unwrap();
        assert_eq!(migrated.choice, "dark");
        assert_eq!(migrated.background, "glass");
        assert_eq!(migrated.panes, "rounded");
        assert_eq!(load_settings(&db, work.id).unwrap(), ThemeSettings::default());
    }

    #[test]
    fn theme_settings_default_persist_and_validate() {
        let db = Database::open_memory().unwrap();
        assert_eq!(load_settings(&db, default_space(&db)).unwrap(), ThemeSettings::default());

        let settings = ThemeSettings {
            choice: "catppuccin".into(),
            accent: Some("red".into()),
            background: "glass".into(),
            panes: "rounded".into(),
        };
        save_settings(&db, default_space(&db), &settings).unwrap();
        assert_eq!(load_settings(&db, default_space(&db)).unwrap(), settings);
        assert!(!matugen_enabled(&db, default_space(&db)).unwrap());

        let matugen = ThemeSettings {
            choice: "matugen".into(),
            accent: None,
            background: "solid".into(),
            panes: "straight".into(),
        };
        save_settings(&db, default_space(&db), &matugen).unwrap();
        assert!(matugen_enabled(&db, default_space(&db)).unwrap());

        let invalid = ThemeSettings {
            choice: "sepia".into(),
            ..ThemeSettings::default()
        };
        assert!(save_settings(&db, default_space(&db), &invalid).is_err());
        assert_eq!(load_settings(&db, default_space(&db)).unwrap(), matugen);

        let invalid_background = ThemeSettings {
            background: "liquid".into(),
            ..ThemeSettings::default()
        };
        assert!(save_settings(&db, default_space(&db), &invalid_background).is_err());
        assert_eq!(load_settings(&db, default_space(&db)).unwrap(), matugen);

        let invalid_panes = ThemeSettings {
            panes: "bubbles".into(),
            ..ThemeSettings::default()
        };
        assert!(save_settings(&db, default_space(&db), &invalid_panes).is_err());
        assert_eq!(load_settings(&db, default_space(&db)).unwrap(), matugen);
    }

    #[test]
    fn migrates_legacy_theme_styles_to_shape_choices() {
        for (style, background, panes) in [
            ("classic", "solid", "straight"),
            ("frosted", "glass", "straight"),
            ("fancy", "glass", "rounded"),
        ] {
            let db = Database::open_memory().unwrap();
            db.set_setting(STYLE_KEY, style).unwrap();

            let settings = load_settings(&db, default_space(&db)).unwrap();
            assert_eq!(settings.background, background);
            assert_eq!(settings.panes, panes);
        }
    }

    #[test]
    fn migrates_legacy_theme_settings() {
        let db = Database::open_memory().unwrap();
        db.set_setting(MODE_KEY, "dark").unwrap();
        db.set_setting(DARK_PRESET_KEY, "dracula").unwrap();
        assert_eq!(load_settings(&db, default_space(&db)).unwrap().choice, "dracula");

        let db = Database::open_memory().unwrap();
        db.set_setting(MODE_KEY, "light").unwrap();
        db.set_setting(LIGHT_PRESET_KEY, "rose-pine-dawn").unwrap();
        db.set_setting(DARK_PRESET_KEY, "rose-pine-moon").unwrap();
        assert_eq!(load_settings(&db, default_space(&db)).unwrap().choice, "rose-pine");

        let db = Database::open_memory().unwrap();
        db.set_setting(MODE_KEY, "dark").unwrap();
        db.set_setting(MATUGEN_KEY, "true").unwrap();
        assert_eq!(load_settings(&db, default_space(&db)).unwrap().choice, "matugen");
        assert!(matugen_enabled(&db, default_space(&db)).unwrap());
    }

    #[test]
    fn creates_config_dir_and_template_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("auramail");

        ensure_config_at(&config).unwrap();

        assert!(config.is_dir());
        let template = std::fs::read_to_string(config.join("theme_template.json")).unwrap();
        assert_eq!(template, BUILT_IN_TEMPLATE);
    }

    #[test]
    fn preserves_an_existing_template() {
        let dir = tempfile::tempdir().unwrap();
        let template = dir.path().join("theme_template.json");
        std::fs::write(&template, "custom template").unwrap();

        ensure_config_at(dir.path()).unwrap();

        assert_eq!(
            std::fs::read_to_string(template).unwrap(),
            "custom template"
        );
    }

    #[test]
    fn corrupt_stored_choice_falls_back_to_system() {
        let db = Database::open_memory().unwrap();
        db.set_setting(CHOICE_KEY, "sepia").unwrap();
        let settings = load_settings(&db, default_space(&db)).unwrap();
        assert_eq!(settings.choice, "system");
        assert_eq!(settings.accent, None);
    }
}
