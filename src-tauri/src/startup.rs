use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::error::{AppError, AppResult};

/// Unscoped: launch at login belongs to the installed app, not to a space.
pub const START_HIDDEN_KEY: &str = "desktop.start_hidden";
/// Fixed at plugin registration; it cannot carry "start hidden" as well.
pub const AUTOSTART_FLAG: &str = "--autostart";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupPreferences {
    pub supported: bool,
    pub launch_at_login: bool,
    pub start_hidden: bool,
}

pub fn has_autostart_flag<I: IntoIterator<Item = String>>(args: I) -> bool {
    args.into_iter().any(|arg| arg == AUTOSTART_FLAG)
}

pub fn is_autostart_launch() -> bool {
    has_autostart_flag(std::env::args())
}

pub fn should_start_hidden(from_autostart: bool, start_hidden: bool) -> bool {
    from_autostart && start_hidden
}

fn read_start_hidden(db: &Database) -> AppResult<bool> {
    Ok(matches!(
        db.setting(START_HIDDEN_KEY)?.as_deref(),
        Some("true") | Some("1")
    ))
}

fn write_start_hidden(db: &Database, value: bool) -> AppResult<()> {
    db.set_setting(START_HIDDEN_KEY, if value { "true" } else { "false" })
}

/// Applies the OS change first: a failed `enable()`/`disable()` must not leave
/// the stored flag claiming a state the OS never accepted.
fn commit(os_change: AppResult<()>, db: &Database, start_hidden: bool) -> AppResult<()> {
    os_change?;
    write_start_hidden(db, start_hidden)
}

#[cfg(desktop)]
fn apply_launch_at_login(app: &tauri::AppHandle, enabled: bool) -> AppResult<()> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|error| AppError::Message(format!("launch at login: {error}")))
}

/// Mobile has no login items; the flag is still stored so the payload stays uniform.
#[cfg(not(desktop))]
fn apply_launch_at_login(_app: &tauri::AppHandle, _enabled: bool) -> AppResult<()> {
    Ok(())
}

#[cfg(desktop)]
fn launch_at_login(app: &tauri::AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[cfg(not(desktop))]
fn launch_at_login(_app: &tauri::AppHandle) -> bool {
    false
}

pub fn preferences(app: &tauri::AppHandle, db: &Database) -> AppResult<StartupPreferences> {
    Ok(StartupPreferences {
        supported: cfg!(desktop),
        launch_at_login: launch_at_login(app),
        start_hidden: read_start_hidden(db)?,
    })
}

pub fn set_preferences(
    app: &tauri::AppHandle,
    db: &Database,
    next: StartupPreferences,
) -> AppResult<StartupPreferences> {
    commit(
        apply_launch_at_login(app, next.launch_at_login),
        db,
        next.start_hidden,
    )?;
    preferences(app, db)
}

/// The only place that shows the main window on startup: no flash, one owner.
pub fn apply_window_visibility(app: &tauri::AppHandle, db: &Database) -> AppResult<()> {
    use tauri::Manager;
    if should_start_hidden(is_autostart_launch(), read_start_hidden(db)?) {
        return Ok(());
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::error::AppError;

    #[test]
    fn start_hidden_flag_round_trips_and_defaults_to_false() {
        let db = Database::open_memory().unwrap();
        assert!(!read_start_hidden(&db).unwrap());
        write_start_hidden(&db, true).unwrap();
        assert!(read_start_hidden(&db).unwrap());
        assert_eq!(
            db.setting(START_HIDDEN_KEY).unwrap().as_deref(),
            Some("true")
        );
        write_start_hidden(&db, false).unwrap();
        assert!(!read_start_hidden(&db).unwrap());
    }

    #[test]
    fn an_unrecognised_flag_value_reads_as_false() {
        let db = Database::open_memory().unwrap();
        db.set_setting(START_HIDDEN_KEY, "yes").unwrap();
        assert!(!read_start_hidden(&db).unwrap());
    }

    #[test]
    fn hidden_start_needs_both_the_marker_and_the_flag() {
        assert!(!should_start_hidden(false, false));
        assert!(!should_start_hidden(false, true));
        assert!(!should_start_hidden(true, false));
        assert!(should_start_hidden(true, true));
    }

    #[test]
    fn the_autostart_marker_is_matched_exactly() {
        assert!(has_autostart_flag(["--autostart".to_string()]));
        assert!(has_autostart_flag([
            "/usr/bin/auramail".to_string(),
            "--autostart".to_string()
        ]));
        assert!(!has_autostart_flag(["--autostart=1".to_string()]));
        assert!(!has_autostart_flag([std::env::args()
            .next()
            .unwrap_or_default()]));
    }

    #[test]
    fn a_failed_os_change_leaves_the_flag_untouched() {
        let db = Database::open_memory().unwrap();
        let result = commit(Err(AppError::Message("os said no".into())), &db, true);
        assert!(result.is_err());
        assert!(!read_start_hidden(&db).unwrap());
    }

    #[test]
    fn a_successful_os_change_writes_the_flag() {
        let db = Database::open_memory().unwrap();
        commit(Ok(()), &db, true).unwrap();
        assert!(read_start_hidden(&db).unwrap());
        commit(Ok(()), &db, false).unwrap();
        assert!(!read_start_hidden(&db).unwrap());
    }

    #[test]
    fn preferences_serialize_with_camel_case_keys() {
        let value = serde_json::to_value(StartupPreferences {
            supported: true,
            launch_at_login: true,
            start_hidden: false,
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "supported": true,
                "launchAtLogin": true,
                "startHidden": false
            })
        );
    }
}
