use std::sync::Arc;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

use crate::accounts::{self, Account};
use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::imap::client;
use crate::mail::{self, parse, render, MessageDetail};
use crate::secrets::SecretStore;
use crate::spaces;

const MAX_REMOTE_IMAGES: usize = 12;
const MAX_REMOTE_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const PRELOAD_BATCH: usize = 10;
pub const TRUSTED_SENDERS_KEY: &str = "mail.trusted_senders";
pub const ALWAYS_LOAD_IMAGES_KEY: &str = "mail.always_load_images";
pub const PROFILE_PICTURES_KEY: &str = "mail.profile_pictures";

fn scoped_key(space_id: i64, key: &str) -> String {
    format!("space:{space_id}:{key}")
}

fn is_default_space(db: &Database, space_id: i64) -> bool {
    db.conn
        .lock()
        .unwrap()
        .query_row(
            "select lower(name) in ('default', 'personal') from spaces where id = ?1",
            rusqlite::params![space_id],
            |row| row.get(0),
        )
        .unwrap_or(false)
}

fn setting_for_space(
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

pub fn load_detail(
    db: &Database,
    secrets: &dyn SecretStore,
    message_id: i64,
) -> AppResult<MessageDetail> {
    let cached = mail::get_message_detail(db, message_id)?;
    if cached.message.body_cached {
        return Ok(cached);
    }
    let message = cached.message;
    let account = account_by_id(db, message.account_id)?;
    let folder = mail::folder_by_id(db, message.folder_id)?
        .ok_or_else(|| AppError::Message("folder not found".into()))?;
    let credential = crate::mail_auth::credential_for(&account, secrets)?;
    let raw = client::fetch_message(&account, &credential, &folder.path, message.uid)?;
    let mut parsed = parse::parse_message(&raw);
    let mut used = Vec::new();
    if let Some(html) = parsed.html_body.take() {
        let (inlined, rendered) = render::inline_cid_images(&html, &parsed.attachments);
        used = rendered;
        parsed.html_body = Some(render::sanitize_html(&inlined));
    }
    remove_inline_attachments(&mut parsed.attachments, &used);
    mail::cache_message_body(db, message_id, &parsed)
}

pub async fn load_images(db: &Database, message_id: i64) -> AppResult<String> {
    let detail = mail::get_message_detail(db, message_id)?;
    if let Some(html) = detail.image_html {
        return Ok(html);
    }
    let html = detail
        .html_body
        .ok_or_else(|| AppError::Message("message has no HTML body".into()))?;
    let images = render::remote_image_sources(&html);
    if images.is_empty() {
        mail::cache_image_html(db, message_id, &html)?;
        return Ok(html);
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|_| AppError::Message("image loader is unavailable".into()))?;
    let mut replacements = Vec::new();
    for image in images.into_iter().take(MAX_REMOTE_IMAGES) {
        if let Some((mime, bytes)) = fetch_image(&client, &image.url).await {
            let data = format!("data:{mime};base64,{}", BASE64.encode(bytes));
            replacements.push((image.start, image.end, data));
        }
    }
    let html = render::replace_ranges(&html, &replacements);
    mail::cache_image_html(db, message_id, &html)?;
    Ok(html)
}

pub fn schedule_preload(db: Arc<Database>, secrets: Arc<dyn SecretStore>, message_ids: Vec<i64>) {
    if message_ids.is_empty() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut ids = message_ids;
        ids.truncate(PRELOAD_BATCH);
        for message_id in ids {
            let task_db = Arc::clone(&db);
            let task_secrets = Arc::clone(&secrets);
            let detail = tauri::async_runtime::spawn_blocking(move || {
                load_detail(&task_db, task_secrets.as_ref(), message_id)
            })
            .await;
            // Preloading is only for the body. Remote images are loaded when
            // the message is opened so a background sync cannot turn into a
            // large image-download and cache-write burst.
            let _ = detail;
        }
    });
}

pub fn trusted_senders(db: &Database, space_id: i64) -> AppResult<Vec<String>> {
    let Some(raw) = setting_for_space(db, space_id, TRUSTED_SENDERS_KEY)? else {
        return Ok(Vec::new());
    };
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

pub fn always_load_images(db: &Database, space_id: i64) -> AppResult<bool> {
    Ok(matches!(
        setting_for_space(db, space_id, ALWAYS_LOAD_IMAGES_KEY)?.as_deref(),
        Some("true") | Some("1")
    ))
}

pub fn profile_pictures(db: &Database, space_id: i64) -> AppResult<bool> {
    Ok(!matches!(
        setting_for_space(db, space_id, PROFILE_PICTURES_KEY)?.as_deref(),
        Some("false") | Some("0")
    ))
}

pub fn set_preferences(
    db: &Database,
    space_id: i64,
    trusted_senders: &[String],
    always_load_images: bool,
    profile_pictures: bool,
) -> AppResult<()> {
    spaces::by_id(db, space_id)?;
    let mut normalized = trusted_senders
        .iter()
        .map(|sender| sender.trim().to_ascii_lowercase())
        .filter(|sender| !sender.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    db.set_setting(
        &scoped_key(space_id, TRUSTED_SENDERS_KEY),
        &serde_json::to_string(&normalized)?,
    )?;
    db.set_setting(
        &scoped_key(space_id, ALWAYS_LOAD_IMAGES_KEY),
        if always_load_images { "true" } else { "false" },
    )?;
    db.set_setting(
        &scoped_key(space_id, PROFILE_PICTURES_KEY),
        if profile_pictures { "true" } else { "false" },
    )?;
    Ok(())
}

fn account_by_id(db: &Database, account_id: i64) -> AppResult<Account> {
    accounts::list(db)?
        .into_iter()
        .find(|account| account.id == account_id)
        .ok_or_else(|| AppError::Message("account not found".into()))
}

fn remove_inline_attachments(
    attachments: &mut Vec<parse::ParsedAttachment>,
    used_content_ids: &[String],
) {
    let used = used_content_ids
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    attachments.retain(|attachment| {
        if attachment.is_inline {
            return false;
        }
        attachment
            .content_id
            .as_deref()
            .map(|content_id| {
                let normalized = content_id
                    .trim()
                    .trim_start_matches("cid:")
                    .trim_matches(['<', '>'])
                    .to_ascii_lowercase();
                !used.contains(&normalized)
            })
            .unwrap_or(true)
    });
}

fn image_mime(content_type: Option<&str>, url: &str) -> Option<&'static str> {
    if let Some(value) = content_type {
        let mime = value
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        return match mime.as_str() {
            "image/png" => Some("image/png"),
            "image/jpeg" | "image/jpg" => Some("image/jpeg"),
            "image/gif" => Some("image/gif"),
            "image/webp" => Some("image/webp"),
            "image/bmp" => Some("image/bmp"),
            "image/avif" => Some("image/avif"),
            _ => None,
        };
    }
    let lower = url.to_ascii_lowercase();
    [
        (".png", "image/png"),
        (".jpg", "image/jpeg"),
        (".jpeg", "image/jpeg"),
        (".gif", "image/gif"),
        (".webp", "image/webp"),
        (".bmp", "image/bmp"),
        (".avif", "image/avif"),
    ]
    .into_iter()
    .find_map(|(extension, mime)| lower.contains(extension).then_some(mime))
}

async fn fetch_image(client: &reqwest::Client, url: &str) -> Option<(String, Vec<u8>)> {
    let parsed = reqwest::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let response = client.get(parsed).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_REMOTE_IMAGE_BYTES as u64)
    {
        return None;
    }
    let mime = image_mime(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        url,
    )?;
    let bytes = response.bytes().await.ok()?;
    if bytes.len() > MAX_REMOTE_IMAGE_BYTES {
        return None;
    }
    Some((mime.to_string(), bytes.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::{image_mime, remove_inline_attachments, set_preferences, trusted_senders};
    use crate::db::Database;
    use crate::mail::parse::ParsedAttachment;

    fn attachment(filename: &str, content_id: Option<&str>) -> ParsedAttachment {
        ParsedAttachment {
            filename: filename.into(),
            mime_type: "image/png".into(),
            size: 3,
            content_id: content_id.map(str::to_string),
            is_inline: content_id.is_some(),
            data: vec![1, 2, 3],
        }
    }

    #[test]
    fn image_mime_requires_a_known_image_type() {
        assert_eq!(
            image_mime(Some("image/png; charset=binary"), ""),
            Some("image/png")
        );
        assert_eq!(image_mime(Some("image/svg+xml"), ""), None);
        assert_eq!(
            image_mime(None, "https://example.com/a.webp"),
            Some("image/webp")
        );
        assert_eq!(image_mime(None, "https://example.com/a.html"), None);
    }

    #[test]
    fn removes_inline_attachments_and_rendered_cid_parts() {
        let mut attachments = vec![
            attachment("logo.png", Some("<logo@example>")),
            attachment("invoice.pdf", None),
            attachment("unused.png", Some("unused@example")),
        ];
        remove_inline_attachments(&mut attachments, &["logo@example".into()]);
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].filename, "invoice.pdf");
    }

    #[test]
    fn mail_preferences_are_isolated_per_space() {
        let db = Database::open_memory().unwrap();
        let default = crate::spaces::ensure_default(&db).unwrap();
        let work = crate::spaces::create(&db, "Work").unwrap();
        set_preferences(
            &db,
            default.id,
            &["ada@example.com".into()],
            true,
            false,
        )
        .unwrap();
        assert_eq!(
            trusted_senders(&db, default.id).unwrap(),
            vec!["ada@example.com".to_string()]
        );
        assert!(super::always_load_images(&db, default.id).unwrap());
        assert!(!super::profile_pictures(&db, default.id).unwrap());
        assert!(trusted_senders(&db, work.id).unwrap().is_empty());
        assert!(!super::always_load_images(&db, work.id).unwrap());
        assert!(super::profile_pictures(&db, work.id).unwrap());
    }

    #[test]
    fn profile_pictures_default_on() {
        let db = Database::open_memory().unwrap();
        let space_id = crate::spaces::default_id(&db).unwrap();
        assert!(super::profile_pictures(&db, space_id).unwrap());
    }

    #[test]
    fn profile_pictures_persist_opt_out() {
        let db = Database::open_memory().unwrap();
        let space_id = crate::spaces::default_id(&db).unwrap();
        set_preferences(&db, space_id, &[], false, false).unwrap();
        assert!(!super::profile_pictures(&db, space_id).unwrap());
    }

    #[test]
    fn trusted_senders_are_normalized_and_persisted() {
        let db = Database::open_memory().unwrap();
        let space_id = crate::spaces::default_id(&db).unwrap();
        set_preferences(
            &db,
            space_id,
            &["  ADA@Example.com ".into(), "ada@example.com".into()],
            true,
            true,
        )
        .unwrap();
        assert_eq!(
            trusted_senders(&db, space_id).unwrap(),
            vec!["ada@example.com".to_string()]
        );
        assert!(super::always_load_images(&db, space_id).unwrap());
    }
}
