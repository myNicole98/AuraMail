use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};
use tauri_plugin_notification::NotificationExt;

use crate::accounts::Account;
use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::mail;

const NOTIFICATIONS_KEY: &str = "desktop.notifications";
const NEW_MAIL_SOUND_KEY: &str = "desktop.new_mail_sound";
const SENT_MAIL_SOUND_KEY: &str = "desktop.sent_mail_sound";
const CLOSE_ACTION_KEY: &str = "desktop.close_action";
const MINIMIZE_ACTION_KEY: &str = "desktop.minimize_action";

static ACTIVE_SPACE: AtomicI64 = AtomicI64::new(0);

fn scoped_key(space_id: i64, key: &str) -> String {
    format!("space:{space_id}:{key}")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPreferences {
    pub notifications: bool,
    pub new_mail_sound: bool,
    pub sent_mail_sound: bool,
    pub close_action: String,
    pub minimize_action: String,
}

impl Default for DesktopPreferences {
    fn default() -> Self {
        Self {
            notifications: false,
            new_mail_sound: true,
            sent_mail_sound: true,
            close_action: "tray".into(),
            minimize_action: "taskbar".into(),
        }
    }
}

fn setting(db: &Database, space_id: i64, key: &str) -> AppResult<Option<String>> {
    if space_id > 0 {
        if let Some(value) = db.setting(&scoped_key(space_id, key))? {
            return Ok(Some(value));
        }
    }
    // Existing installations keep their previous global desktop preferences
    // until the workspace writes its own value.
    db.setting(key)
}

fn read_bool(db: &Database, space_id: i64, key: &str, fallback: bool) -> AppResult<bool> {
    Ok(match setting(db, space_id, key)?.as_deref() {
        Some("true") | Some("1") => true,
        Some("false") | Some("0") => false,
        _ => fallback,
    })
}

fn read_action(
    db: &Database,
    space_id: i64,
    key: &str,
    fallback: &str,
) -> AppResult<String> {
    Ok(setting(db, space_id, key)?
        .filter(|value| matches!(value.as_str(), "tray" | "quit" | "taskbar"))
        .unwrap_or_else(|| fallback.to_string()))
}

pub fn get_for_space(db: &Database, space_id: i64) -> AppResult<DesktopPreferences> {
    let defaults = DesktopPreferences::default();
    Ok(DesktopPreferences {
        notifications: read_bool(db, space_id, NOTIFICATIONS_KEY, defaults.notifications)?,
        new_mail_sound: read_bool(
            db,
            space_id,
            NEW_MAIL_SOUND_KEY,
            defaults.new_mail_sound,
        )?,
        sent_mail_sound: read_bool(
            db,
            space_id,
            SENT_MAIL_SOUND_KEY,
            defaults.sent_mail_sound,
        )?,
        close_action: read_action(
            db,
            space_id,
            CLOSE_ACTION_KEY,
            &defaults.close_action,
        )?,
        minimize_action: read_action(
            db,
            space_id,
            MINIMIZE_ACTION_KEY,
            &defaults.minimize_action,
        )?,
    })
}

pub fn set_for_space(
    db: &Database,
    space_id: i64,
    preferences: DesktopPreferences,
) -> AppResult<DesktopPreferences> {
    crate::spaces::by_id(db, space_id)?;
    if !matches!(preferences.close_action.as_str(), "tray" | "quit") {
        return Err(AppError::Message("invalid close action".into()));
    }
    if !matches!(preferences.minimize_action.as_str(), "tray" | "taskbar") {
        return Err(AppError::Message("invalid minimize action".into()));
    }
    db.set_setting(
        &scoped_key(space_id, NOTIFICATIONS_KEY),
        if preferences.notifications { "true" } else { "false" },
    )?;
    db.set_setting(
        &scoped_key(space_id, NEW_MAIL_SOUND_KEY),
        if preferences.new_mail_sound { "true" } else { "false" },
    )?;
    db.set_setting(
        &scoped_key(space_id, SENT_MAIL_SOUND_KEY),
        if preferences.sent_mail_sound { "true" } else { "false" },
    )?;
    db.set_setting(
        &scoped_key(space_id, CLOSE_ACTION_KEY),
        &preferences.close_action,
    )?;
    db.set_setting(
        &scoped_key(space_id, MINIMIZE_ACTION_KEY),
        &preferences.minimize_action,
    )?;
    Ok(preferences)
}

pub fn get(db: &Database) -> AppResult<DesktopPreferences> {
    get_for_space(db, 0)
}

pub fn set(db: &Database, preferences: DesktopPreferences) -> AppResult<DesktopPreferences> {
    if !matches!(preferences.close_action.as_str(), "tray" | "quit") {
        return Err(AppError::Message("invalid close action".into()));
    }
    if !matches!(preferences.minimize_action.as_str(), "tray" | "taskbar") {
        return Err(AppError::Message("invalid minimize action".into()));
    }
    db.set_setting(
        NOTIFICATIONS_KEY,
        if preferences.notifications { "true" } else { "false" },
    )?;
    db.set_setting(
        NEW_MAIL_SOUND_KEY,
        if preferences.new_mail_sound { "true" } else { "false" },
    )?;
    db.set_setting(
        SENT_MAIL_SOUND_KEY,
        if preferences.sent_mail_sound { "true" } else { "false" },
    )?;
    db.set_setting(CLOSE_ACTION_KEY, &preferences.close_action)?;
    db.set_setting(MINIMIZE_ACTION_KEY, &preferences.minimize_action)?;
    Ok(preferences)
}

pub fn set_active_space(space_id: i64) {
    ACTIVE_SPACE.store(space_id, Ordering::SeqCst);
}

fn active_space() -> i64 {
    ACTIVE_SPACE.load(Ordering::SeqCst)
}

pub fn close_to_tray(db: &Database) -> bool {
    get_for_space(db, active_space())
        .map(|preferences| preferences.close_action == "tray")
        .unwrap_or(true)
}

pub fn minimize_to_tray(db: &Database) -> bool {
    get_for_space(db, active_space())
        .map(|preferences| preferences.minimize_action == "tray")
        .unwrap_or(false)
}

pub fn notify_new_mail(app: &AppHandle, db: &Database, account: &Account, message_ids: &[i64]) {
    let enabled = get_for_space(db, account.space_id)
        .map(|preferences| preferences.notifications)
        .unwrap_or(false);
    if !enabled || message_ids.is_empty() {
        return;
    }
    let Some(first) = message_ids
        .iter()
        .find_map(|id| mail::message_by_id(db, *id).ok())
    else {
        return;
    };
    let sender = if first.from_name.trim().is_empty() {
        first.from_addr.clone()
    } else {
        first.from_name.clone()
    };
    let subject = if first.subject.trim().is_empty() {
        "(No subject)".to_string()
    } else {
        first.subject.clone()
    };
    let (title, body) = if message_ids.len() == 1 {
        (sender, format!("{subject} · {}", account.display_name))
    } else {
        (
            format!("{} new messages", message_ids.len()),
            format!("{sender} · {subject} · {}", account.display_name),
        )
    };
    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .action_type_id("auramail.mail")
        .extra("spaceId", account.space_id)
        .extra("messageId", first.id)
        .show();
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SpaceOpen {
    space_id: i64,
    message_id: i64,
}

/// Routes a notification click back to the window that owns the message.
pub fn install_notification_actions(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.notification().on_action(move |action| {
        if !matches!(action.action_id(), "tap" | "open") {
            return;
        }
        let Some(notification) = action.notification() else {
            return;
        };
        let extra = notification.extra();
        let space_id = extra
            .get("spaceId")
            .or_else(|| extra.get("space_id"))
            .and_then(|value| value.as_i64());
        let message_id = extra
            .get("messageId")
            .or_else(|| extra.get("message_id"))
            .and_then(|value| value.as_i64());
        let (Some(space_id), Some(message_id)) = (space_id, message_id) else {
            return;
        };
        show_main(&handle);
        let _ = handle.emit(
            "space://open",
            SpaceOpen {
                space_id,
                message_id,
            },
        );
    });
}

static TRAY_UNREAD: AtomicBool = AtomicBool::new(false);

fn tray_image(unread: bool) -> tauri::Result<tauri::image::Image<'static>> {
    let bytes = if unread {
        include_bytes!("../assets/tray-inbox-unread.png").as_slice()
    } else {
        include_bytes!("../assets/tray-inbox.png").as_slice()
    };
    tauri::image::Image::from_bytes(bytes)
}

pub fn set_tray_unread(app: &AppHandle, unread: bool) {
    if TRAY_UNREAD.swap(unread, Ordering::Relaxed) == unread {
        return;
    }
    let Ok(image) = tray_image(unread) else {
        return;
    };
    if let Some(tray) = app.tray_by_id("main-tray") {
        let _ = tray.set_icon(Some(image));
    }
}

pub fn refresh_tray_unread(app: &AppHandle, db: &Database) {
    let unread = crate::mail::unread_inbox_count(db).unwrap_or(0) > 0;
    set_tray_unread(app, unread);
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(target_os = "linux")]
static TRAY_LIBRARY: OnceLock<libloading::Library> = OnceLock::new();

#[cfg(target_os = "linux")]
fn appindicator_names() -> [&'static str; 4] {
    [
        "libayatana-appindicator3.so.1",
        "libappindicator3.so.1",
        "libayatana-appindicator3.so",
        "libappindicator3.so",
    ]
}

#[cfg(target_os = "linux")]
fn find_appindicator() -> Option<PathBuf> {
    let names = appindicator_names();
    for name in names {
        if let Ok(library) = unsafe { libloading::Library::new(name) } {
            let _ = TRAY_LIBRARY.set(library);
            return Some(PathBuf::from(name));
        }
    }
    let mut dirs = Vec::new();
    if let Ok(path) = std::env::var("LD_LIBRARY_PATH") {
        dirs.extend(path.split(':').filter(|part| !part.is_empty()).map(PathBuf::from));
    }
    if Path::new("/nix/store").is_dir() {
        if let Ok(entries) = std::fs::read_dir("/nix/store") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.contains("libayatana-appindicator") || name.contains("libappindicator") {
                    dirs.push(entry.path().join("lib"));
                }
            }
        }
    }
    dirs.extend([
        PathBuf::from("/usr/lib/x86_64-linux-gnu"),
        PathBuf::from("/usr/lib64"),
        PathBuf::from("/usr/lib"),
        PathBuf::from("/lib/x86_64-linux-gnu"),
    ]);
    for dir in dirs {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                if let Ok(library) = unsafe { libloading::Library::new(&candidate) } {
                    let _ = TRAY_LIBRARY.set(library);
                    return Some(candidate);
                }
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
pub fn tray_available() -> bool {
    TRAY_LIBRARY.get().is_some() || find_appindicator().is_some()
}

pub fn install_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show AuraMail", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit AuraMail", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let icon = tray_image(false)?;

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("AuraMail")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn write_sound(name: &str, bytes: &[u8]) -> Option<PathBuf> {
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

fn play_path(path: &Path) -> bool {
    #[cfg(target_os = "linux")]
    {
        for command in ["pw-play", "paplay", "aplay", "ffplay"] {
            let mut process = Command::new(command);
            if command == "aplay" {
                process.arg("-q");
            } else if command == "ffplay" {
                process.args(["-nodisp", "-autoexit", "-loglevel", "quiet"]);
            }
            if process.arg(path).status().map(|status| status.success()).unwrap_or(false) {
                return true;
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if Command::new("afplay").arg(path).status().map(|status| status.success()).unwrap_or(false) {
            return true;
        }
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Media::Audio::{
            PlaySoundW, SND_FILENAME, SND_NODEFAULT,
        };

        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let played = unsafe {
            PlaySoundW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                SND_FILENAME | SND_NODEFAULT,
            )
        };
        if played != 0 {
            return true;
        }
    }
    false
}

fn play_sound(name: &'static str, bytes: &'static [u8]) {
    std::thread::spawn(move || {
        if let Some(path) = write_sound(name, bytes) {
            let _ = play_path(&path);
        }
    });
}

pub fn play_new_mail_sound() {
    play_sound("auramail-new-mail.wav", include_bytes!("../../public/sounds/new-mail.wav"));
}

pub fn play_sent_sound() {
    play_sound("auramail-sent-mail.wav", include_bytes!("../../public/sounds/sent-mail.wav"));
}

pub fn play_new_mail_if_enabled(db: &Database, space_id: i64) {
    if get_for_space(db, space_id)
        .map(|preferences| preferences.new_mail_sound)
        .unwrap_or(false)
    {
        play_new_mail_sound();
    }
}

pub fn play_sent_if_enabled(db: &Database, space_id: i64) {
    if get_for_space(db, space_id)
        .map(|preferences| preferences.sent_mail_sound)
        .unwrap_or(false)
    {
        play_sent_sound();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn defaults_match_the_ui() {
        let db = Database::open_memory().unwrap();
        let preferences = get(&db).unwrap();
        assert!(!preferences.notifications);
        assert!(preferences.new_mail_sound);
        assert!(preferences.sent_mail_sound);
        assert_eq!(preferences.close_action, "tray");
        assert_eq!(preferences.minimize_action, "taskbar");
        assert!(close_to_tray(&db));
        assert!(!minimize_to_tray(&db));
    }

    #[test]
    fn preferences_are_isolated_per_space() {
        let db = Database::open_memory().unwrap();
        let default = crate::spaces::ensure_default(&db).unwrap();
        let work = crate::spaces::create(&db, "Work").unwrap();
        let next = DesktopPreferences {
            notifications: true,
            new_mail_sound: false,
            sent_mail_sound: false,
            close_action: "quit".into(),
            minimize_action: "tray".into(),
        };

        assert_eq!(set_for_space(&db, work.id, next.clone()).unwrap(), next);
        assert_eq!(get_for_space(&db, work.id).unwrap(), next);
        assert_eq!(
            get_for_space(&db, default.id).unwrap(),
            DesktopPreferences::default()
        );
    }

    #[test]
    fn preferences_round_trip_and_validate_actions() {
        let db = Database::open_memory().unwrap();
        let next = DesktopPreferences {
            notifications: true,
            new_mail_sound: false,
            sent_mail_sound: false,
            close_action: "quit".into(),
            minimize_action: "tray".into(),
        };
        assert_eq!(set(&db, next.clone()).unwrap(), next);
        assert_eq!(get(&db).unwrap(), next);
        assert!(!close_to_tray(&db));
        assert!(minimize_to_tray(&db));

        let invalid = DesktopPreferences {
            close_action: "minimize".into(),
            ..next
        };
        assert!(set(&db, invalid).is_err());
    }
}
