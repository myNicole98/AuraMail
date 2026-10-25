use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use hickory_resolver::TokioAsyncResolver;
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::db::Database;
use crate::error::AppResult;
use crate::mail::preload;

const MISS_TTL_SECONDS: i64 = 24 * 60 * 60;
const MAX_AVATAR_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarData {
    pub mime: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedAvatar {
    pub source: String,
    pub data: Option<AvatarData>,
    pub fetched_at: i64,
}

pub fn normalize_email(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

pub fn gravatar_url(email: &str) -> String {
    let digest = Sha256::digest(normalize_email(email).as_bytes());
    let hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("https://gravatar.com/avatar/{hash}?s=128&d=404")
}

pub fn parse_bimi_record(record: &str) -> Option<String> {
    let mut version = false;
    let mut logo = None;
    for field in record.split(';') {
        let Some((key, value)) = field.split_once('=') else {
            continue;
        };
        match key.trim().to_ascii_lowercase().as_str() {
            "v" => version = value.trim() == "BIMI1",
            "l" => logo = Some(value.trim().to_string()),
            _ => {}
        }
    }
    let logo = logo?;
    (version && logo.to_ascii_lowercase().starts_with("https://")).then_some(logo)
}

pub fn data_url(avatar: &AvatarData) -> String {
    format!(
        "data:{};base64,{}",
        avatar.mime,
        BASE64.encode(&avatar.bytes)
    )
}

pub fn miss_is_fresh(fetched_at: i64, now: i64) -> bool {
    now.saturating_sub(fetched_at) < MISS_TTL_SECONDS
}

fn cache_lookup(db: &Database, email: &str) -> AppResult<Option<CachedAvatar>> {
    let conn = db.conn.lock().unwrap();
    let row = conn
        .query_row(
            "select source, mime, bytes, fetched_at from sender_avatars where email = ?1",
            params![email],
            |row| {
                let source: String = row.get(0)?;
                let mime: Option<String> = row.get(1)?;
                let bytes: Option<Vec<u8>> = row.get(2)?;
                let fetched_at: i64 = row.get(3)?;
                Ok(CachedAvatar {
                    source,
                    data: mime.zip(bytes).map(|(mime, bytes)| AvatarData { mime, bytes }),
                    fetched_at,
                })
            },
        )
        .optional()?;
    Ok(row)
}

fn cache_hit(
    db: &Database,
    email: &str,
    source: &str,
    avatar: &AvatarData,
    now: i64,
) -> AppResult<()> {
    db.conn.lock().unwrap().execute(
        "insert into sender_avatars (email, source, mime, bytes, fetched_at)
         values (?1, ?2, ?3, ?4, ?5)
         on conflict(email) do update set
           source = excluded.source,
           mime = excluded.mime,
           bytes = excluded.bytes,
           fetched_at = excluded.fetched_at",
        params![email, source, avatar.mime, avatar.bytes, now],
    )?;
    Ok(())
}

fn cache_miss(db: &Database, email: &str, now: i64) -> AppResult<()> {
    db.conn.lock().unwrap().execute(
        "insert into sender_avatars (email, source, mime, bytes, fetched_at)
         values (?1, 'missing', null, null, ?2)
         on conflict(email) do update set
           source = 'missing',
           mime = null,
           bytes = null,
           fetched_at = excluded.fetched_at",
        params![email, now],
    )?;
    Ok(())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

pub async fn resolve(
    db: &Database,
    email: &str,
    space_id: i64,
) -> AppResult<Option<AvatarData>> {
    if !preload::profile_pictures(db, space_id)? {
        return Ok(None);
    }
    let email = normalize_email(email);
    if email.is_empty() {
        return Ok(None);
    }
    let now = unix_now();
    if let Some(cached) = cache_lookup(db, &email)? {
        if let Some(avatar) = cached.data {
            return Ok(Some(avatar));
        }
        if cached.source == "missing" && miss_is_fresh(cached.fetched_at, now) {
            return Ok(None);
        }
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|_| crate::error::AppError::Message("avatar loader is unavailable".into()))?;

    if let Some(domain) = email.split_once('@').map(|(_, domain)| domain) {
        if let Some(logo) = bimi_logo_url(domain).await {
            if let Some(avatar) = fetch_avatar(&client, &logo).await {
                cache_hit(db, &email, "bimi", &avatar, now)?;
                return Ok(Some(avatar));
            }
        }
    }

    if let Some(avatar) = fetch_avatar(&client, &gravatar_url(&email)).await {
        cache_hit(db, &email, "gravatar", &avatar, now)?;
        return Ok(Some(avatar));
    }

    cache_miss(db, &email, now)?;
    Ok(None)
}

async fn bimi_logo_url(domain: &str) -> Option<String> {
    let resolver = TokioAsyncResolver::tokio_from_system_conf().ok()?;
    let lookup = resolver
        .txt_lookup(format!("default._bimi.{domain}"))
        .await
        .ok()?;
    for record in lookup.iter() {
        let text = record
            .txt_data()
            .iter()
            .map(|chunk| String::from_utf8_lossy(chunk))
            .collect::<String>();
        if let Some(logo) = parse_bimi_record(&text) {
            return Some(logo);
        }
    }
    None
}

async fn fetch_avatar(client: &reqwest::Client, url: &str) -> Option<AvatarData> {
    let parsed = reqwest::Url::parse(url).ok()?;
    if parsed.scheme() != "https" {
        return None;
    }
    let response = client.get(parsed).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_AVATAR_BYTES as u64)
    {
        return None;
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    let mime = avatar_mime(content_type, url)?;
    let bytes = response.bytes().await.ok()?;
    if bytes.len() > MAX_AVATAR_BYTES {
        return None;
    }
    Some(AvatarData {
        mime: mime.to_string(),
        bytes: bytes.to_vec(),
    })
}

fn avatar_mime(content_type: Option<&str>, url: &str) -> Option<&'static str> {
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
            "image/svg+xml" => Some("image/svg+xml"),
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
        (".svg", "image/svg+xml"),
    ]
    .into_iter()
    .find_map(|(extension, mime)| lower.contains(extension).then_some(mime))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_sender_addresses() {
        assert_eq!(normalize_email("  Ada@Example.COM "), "ada@example.com");
    }

    #[test]
    fn builds_gravatar_url_from_the_normalized_address() {
        assert_eq!(
            gravatar_url(" Ada@Example.COM "),
            "https://gravatar.com/avatar/b5fc85e55755f9e0d030a10ab4429b6b2944855f9a0d60077fe832becbc41d72?s=128&d=404"
        );
    }

    #[test]
    fn parses_a_bimi_logo_url() {
        assert_eq!(
            parse_bimi_record("v=BIMI1; l=https://example.com/logo.svg; a=self;"),
            Some("https://example.com/logo.svg".into())
        );
    }

    #[test]
    fn rejects_bimi_records_without_a_secure_logo() {
        assert_eq!(parse_bimi_record("v=BIMI1; l=; a=;"), None);
        assert_eq!(
            parse_bimi_record("v=BIMI1; l=http://example.com/logo.svg;"),
            None
        );
        assert_eq!(
            parse_bimi_record("v=OTHER; l=https://example.com/logo.svg;"),
            None
        );
    }

    #[test]
    fn builds_a_data_url() {
        assert_eq!(
            data_url(&AvatarData {
                mime: "image/png".into(),
                bytes: vec![1, 2, 3],
            }),
            "data:image/png;base64,AQID"
        );
    }

    #[test]
    fn cached_hits_and_misses_round_trip() {
        let db = Database::open_memory().unwrap();
        let email = "ada@example.com";
        assert_eq!(cache_lookup(&db, email).unwrap(), None);

        let avatar = AvatarData {
            mime: "image/png".into(),
            bytes: vec![1, 2, 3],
        };
        cache_hit(&db, email, "gravatar", &avatar, 100).unwrap();
        assert_eq!(
            cache_lookup(&db, email).unwrap(),
            Some(CachedAvatar {
                source: "gravatar".into(),
                data: Some(avatar),
                fetched_at: 100,
            })
        );

        cache_miss(&db, "nobody@example.com", 200).unwrap();
        assert_eq!(
            cache_lookup(&db, "nobody@example.com").unwrap(),
            Some(CachedAvatar {
                source: "missing".into(),
                data: None,
                fetched_at: 200,
            })
        );
    }

    #[test]
    fn misses_are_only_fresh_for_a_day() {
        assert!(miss_is_fresh(100, 100 + MISS_TTL_SECONDS - 1));
        assert!(!miss_is_fresh(100, 100 + MISS_TTL_SECONDS));
    }

    #[tokio::test]
    async fn resolves_a_cached_hit() {
        let db = Database::open_memory().unwrap();
        let space_id = crate::spaces::default_id(&db).unwrap();
        let email = "ada@example.com";
        let avatar = AvatarData {
            mime: "image/png".into(),
            bytes: vec![1, 2, 3],
        };
        cache_hit(&db, email, "gravatar", &avatar, unix_now()).unwrap();
        assert_eq!(resolve(&db, email, space_id).await.unwrap(), Some(avatar));
    }

    #[tokio::test]
    async fn respects_a_fresh_cached_miss() {
        let db = Database::open_memory().unwrap();
        let space_id = crate::spaces::default_id(&db).unwrap();
        let email = "ada@example.com";
        cache_miss(&db, email, unix_now()).unwrap();
        assert_eq!(resolve(&db, email, space_id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn returns_nothing_when_profile_pictures_are_disabled() {
        let db = Database::open_memory().unwrap();
        let space_id = crate::spaces::default_id(&db).unwrap();
        let email = "ada@example.com";
        let avatar = AvatarData {
            mime: "image/png".into(),
            bytes: vec![1, 2, 3],
        };
        cache_hit(&db, email, "gravatar", &avatar, unix_now()).unwrap();
        crate::mail::preload::set_preferences(&db, space_id, &[], false, false).unwrap();
        assert_eq!(resolve(&db, email, space_id).await.unwrap(), None);
    }
}
