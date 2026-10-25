use hickory_resolver::TokioAsyncResolver;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::db::{self, Database};
use crate::error::{AppError, AppResult};
use crate::secrets::{account_key, SecretStore};
use crate::spaces;

pub const SYNC_INTERVAL_KEY: &str = "sync.interval_secs";
pub const FAST_SYNC_INTERVAL_KEY: &str = "sync.fast_interval_secs";
pub const DEFAULT_SYNC_INTERVAL_SECS: i64 = 120;
pub const DEFAULT_FAST_SYNC_INTERVAL_SECS: i64 = 30;
const MIN_SYNC_INTERVAL_SECS: i64 = 60;
const MAX_SYNC_INTERVAL_SECS: i64 = 3600;
const MIN_FAST_SYNC_INTERVAL_SECS: i64 = 15;
const MAX_FAST_SYNC_INTERVAL_SECS: i64 = 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MailSecurity {
    Tls,
    StartTls,
}

impl MailSecurity {
    pub fn parse(value: &str) -> AppResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "tls" => Ok(Self::Tls),
            "starttls" | "start_tls" | "start-tls" => Ok(Self::StartTls),
            _ => Err(AppError::Message("security must be tls or starttls".into())),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tls => "tls",
            Self::StartTls => "starttls",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureSocial {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureFields {
    #[serde(default)]
    pub profession: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub website: String,
    #[serde(default)]
    pub socials: Vec<SignatureSocial>,
}

#[derive(Clone, Deserialize)]
pub struct AccountInput {
    #[serde(default)]
    pub display_name: String,
    pub email: String,
    pub username: String,
    pub password: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_security: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_security: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub signature: String,
    #[serde(default)]
    pub signature_fields: SignatureFields,
    #[serde(default = "default_true")]
    pub signature_on_new: bool,
    pub space_id: i64,
}

fn default_true() -> bool {
    true
}

fn display_name_or_email(value: &str, email: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        email.trim().to_string()
    } else {
        value.to_string()
    }
}

pub fn account_color(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() == 7
        && trimmed.starts_with('#')
        && trimmed[1..].chars().all(|character| character.is_ascii_hexdigit())
    {
        return trimmed.to_ascii_lowercase();
    }
    match trimmed {
        "blue" => "blue",
        "lavender" => "lavender",
        "mint" => "mint",
        "peach" => "peach",
        "rose" => "rose",
        "sand" => "sand",
        "sage" => "sage",
        "sky" => "sky",
        "lilac" => "lilac",
        "slate" => "slate",
        "accent" => "accent",
        "accent-alt" => "accent-alt",
        "success" => "success",
        "warning" => "warning",
        "info" => "info",
        "danger" => "danger",
        _ => "blue",
    }
    .into()
}

impl AccountInput {
    fn validate_with_password(&self, password_required: bool) -> AppResult<()> {
        for (name, value) in [
            ("email", &self.email),
            ("username", &self.username),
            ("IMAP host", &self.imap_host),
            ("SMTP host", &self.smtp_host),
        ] {
            if value.trim().is_empty() {
                return Err(AppError::Message(format!("{name} is required")));
            }
        }
        if password_required && self.password.trim().is_empty() {
            return Err(AppError::Message("password is required".into()));
        }
        if self.imap_port == 0 {
            return Err(AppError::Message("IMAP port is required".into()));
        }
        if self.smtp_port == 0 {
            return Err(AppError::Message("SMTP port is required".into()));
        }
        MailSecurity::parse(&self.imap_security)?;
        MailSecurity::parse(&self.smtp_security)?;
        Ok(())
    }

    pub fn validate(&self) -> AppResult<()> {
        self.validate_with_password(true)
    }

    pub fn validate_update(&self) -> AppResult<()> {
        self.validate_with_password(false)
    }
}

#[derive(Clone, Deserialize)]
pub struct OAuthAccountInput {
    #[serde(default)]
    pub display_name: String,
    pub email: String,
    pub username: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_security: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_security: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub provider: String,
    pub space_id: i64,
}

impl OAuthAccountInput {
    pub fn validate(&self) -> AppResult<()> {
        for (name, value) in [
            ("email", &self.email),
            ("username", &self.username),
            ("IMAP host", &self.imap_host),
            ("SMTP host", &self.smtp_host),
        ] {
            if value.trim().is_empty() {
                return Err(AppError::Message(format!("{name} is required")));
            }
        }
        if self.imap_port == 0 {
            return Err(AppError::Message("IMAP port is required".into()));
        }
        if self.smtp_port == 0 {
            return Err(AppError::Message("SMTP port is required".into()));
        }
        MailSecurity::parse(&self.imap_security)?;
        MailSecurity::parse(&self.smtp_security)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccountAuth {
    #[default]
    Password,
    OAuth,
}

fn parse_auth_type(value: &str) -> AccountAuth {
    match value {
        "oauth" => AccountAuth::OAuth,
        _ => AccountAuth::Password,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Account {
    pub id: i64,
    pub display_name: String,
    pub email: String,
    pub username: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_security: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_security: String,
    pub color: String,
    pub signature: String,
    pub signature_fields: SignatureFields,
    pub signature_on_new: bool,
    pub auth_type: AccountAuth,
    pub oauth_provider: String,
    pub space_id: i64,
    pub position: i64,
    pub sync_interval_secs: i64,
    pub created_at: i64,
}

pub fn sync_interval(db: &Database) -> AppResult<i64> {
    let stored = db
        .setting(SYNC_INTERVAL_KEY)?
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_SYNC_INTERVAL_SECS);
    Ok(stored.clamp(MIN_SYNC_INTERVAL_SECS, MAX_SYNC_INTERVAL_SECS))
}

pub fn fast_sync_interval(db: &Database) -> AppResult<i64> {
    let stored = db
        .setting(FAST_SYNC_INTERVAL_KEY)?
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_FAST_SYNC_INTERVAL_SECS);
    Ok(stored.clamp(MIN_FAST_SYNC_INTERVAL_SECS, MAX_FAST_SYNC_INTERVAL_SECS))
}

pub fn set_fast_sync_interval(db: &Database, seconds: i64) -> AppResult<i64> {
    if !(MIN_FAST_SYNC_INTERVAL_SECS..=MAX_FAST_SYNC_INTERVAL_SECS).contains(&seconds) {
        return Err(AppError::Message(
            "inbox sync interval must be between 15 seconds and 60 minutes".into(),
        ));
    }
    db.set_setting(FAST_SYNC_INTERVAL_KEY, &seconds.to_string())?;
    Ok(seconds)
}

pub fn set_sync_interval(db: &Database, seconds: i64) -> AppResult<Vec<Account>> {
    if !(MIN_SYNC_INTERVAL_SECS..=MAX_SYNC_INTERVAL_SECS).contains(&seconds) {
        return Err(AppError::Message(
            "sync interval must be between 1 and 60 minutes".into(),
        ));
    }
    db.set_setting(SYNC_INTERVAL_KEY, &seconds.to_string())?;
    db.conn.lock().unwrap().execute(
        "update accounts set sync_interval_secs = ?1",
        rusqlite::params![seconds],
    )?;
    list(db)
}

pub fn create(db: &Database, secrets: &dyn SecretStore, input: AccountInput) -> AppResult<Account> {
    input.validate()?;
    if find_by_email_ci(db, &input.email)?.is_some() {
        return Err(AppError::Message("account already exists".into()));
    }
    let imap_security = MailSecurity::parse(&input.imap_security)?;
    let smtp_security = MailSecurity::parse(&input.smtp_security)?;
    let created_at = db::now();
    let sync_interval_secs = sync_interval(db)?;

    let color = account_color(&input.color);
    let display_name = display_name_or_email(&input.display_name, &input.email);
    let signature_fields = serde_json::to_string(&input.signature_fields)?;
    let space_id = input.space_id;
    spaces::by_id(db, space_id)?;
    let (id, position) = {
        let conn = db.conn.lock().unwrap();
        let position: i64 = conn.query_row(
            "select coalesce(max(position) + 1, 0) from accounts where space_id = ?1",
            rusqlite::params![space_id],
            |row| row.get(0),
        )?;
        let inserted = conn.execute(
            "insert into accounts (display_name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, sync_interval_secs, color, signature, signature_fields, signature_on_new, auth_type, oauth_provider, space_id, position, created_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            rusqlite::params![
                display_name.clone(),
                input.email,
                input.username,
                input.imap_host,
                input.imap_port,
                imap_security.as_str(),
                input.smtp_host,
                input.smtp_port,
                smtp_security.as_str(),
                sync_interval_secs,
                color.clone(),
                input.signature,
                signature_fields,
                input.signature_on_new,
                "password",
                "",
                space_id,
                position,
                created_at,
            ],
        );
        if let Err(err) = inserted {
            if is_unique_violation(&err) {
                return Err(AppError::Message("account already exists".into()));
            }
            return Err(err.into());
        }
        (conn.last_insert_rowid(), position)
    };

    if let Err(err) = secrets.set(&account_key(id), &input.password) {
        let conn = db.conn.lock().unwrap();
        let _ = conn.execute("delete from accounts where id = ?1", rusqlite::params![id]);
        return Err(err);
    }

    Ok(Account {
        id,
        display_name,
        email: input.email,
        username: input.username,
        imap_host: input.imap_host,
        imap_port: input.imap_port,
        imap_security: imap_security.as_str().into(),
        smtp_host: input.smtp_host,
        smtp_port: input.smtp_port,
        smtp_security: smtp_security.as_str().into(),
        color,
        signature: input.signature,
        signature_fields: input.signature_fields,
        signature_on_new: input.signature_on_new,
        auth_type: AccountAuth::Password,
        oauth_provider: String::new(),
        space_id,
        position,
        sync_interval_secs,
        created_at,
    })
}

pub fn create_oauth(
    db: &Database,
    secrets: &dyn SecretStore,
    input: OAuthAccountInput,
    secret_json: &str,
) -> AppResult<Account> {
    input.validate()?;
    if oauth_identity_taken(db, &input.email, &input.provider)? {
        return Err(AppError::Message("This account is already added.".into()));
    }
    let imap_security = MailSecurity::parse(&input.imap_security)?;
    let smtp_security = MailSecurity::parse(&input.smtp_security)?;
    let created_at = db::now();
    let sync_interval_secs = sync_interval(db)?;
    let color = account_color(&input.color);
    let display_name = display_name_or_email(&input.display_name, &input.email);
    let space_id = input.space_id;
    spaces::by_id(db, space_id)?;

    let (id, position) = {
        let conn = db.conn.lock().unwrap();
        let position: i64 = conn.query_row(
            "select coalesce(max(position) + 1, 0) from accounts where space_id = ?1",
            rusqlite::params![space_id],
            |row| row.get(0),
        )?;
        let inserted = conn.execute(
            "insert into accounts (display_name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, sync_interval_secs, color, signature, signature_fields, signature_on_new, auth_type, oauth_provider, space_id, position, created_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            rusqlite::params![
                display_name.clone(),
                input.email,
                input.username,
                input.imap_host,
                input.imap_port,
                imap_security.as_str(),
                input.smtp_host,
                input.smtp_port,
                smtp_security.as_str(),
                sync_interval_secs,
                color.clone(),
                "",
                "{}",
                true,
                "oauth",
                input.provider,
                space_id,
                position,
                created_at,
            ],
        );
        if let Err(err) = inserted {
            if is_unique_violation(&err) {
                return Err(AppError::Message("This account is already added.".into()));
            }
            return Err(err.into());
        }
        (conn.last_insert_rowid(), position)
    };

    if let Err(err) = secrets.set(&account_key(id), secret_json) {
        let conn = db.conn.lock().unwrap();
        let _ = conn.execute("delete from accounts where id = ?1", rusqlite::params![id]);
        return Err(err);
    }

    Ok(Account {
        id,
        display_name,
        email: input.email,
        username: input.username,
        imap_host: input.imap_host,
        imap_port: input.imap_port,
        imap_security: imap_security.as_str().into(),
        smtp_host: input.smtp_host,
        smtp_port: input.smtp_port,
        smtp_security: smtp_security.as_str().into(),
        color,
        signature: String::new(),
        signature_fields: SignatureFields::default(),
        signature_on_new: true,
        auth_type: AccountAuth::OAuth,
        oauth_provider: input.provider,
        space_id,
        position,
        sync_interval_secs,
        created_at,
    })
}

pub fn update(
    db: &Database,
    secrets: &dyn SecretStore,
    id: i64,
    input: AccountInput,
) -> AppResult<Account> {
    input.validate_update()?;
    let target_space_id = input.space_id;
    spaces::by_id(db, target_space_id)?;
    let imap_security = MailSecurity::parse(&input.imap_security)?;
    let smtp_security = MailSecurity::parse(&input.smtp_security)?;
    let color = account_color(&input.color);
    let display_name = display_name_or_email(&input.display_name, &input.email);
    let signature_fields = serde_json::to_string(&input.signature_fields)?;
    let changed = db.conn.lock().unwrap().execute(
        "update accounts set display_name = ?1, email = ?2, username = ?3, imap_host = ?4, imap_port = ?5, imap_security = ?6, smtp_host = ?7, smtp_port = ?8, smtp_security = ?9, color = ?10, signature = ?11, signature_fields = ?12, signature_on_new = ?13 where id = ?14",
        rusqlite::params![
            display_name.clone(),
            input.email,
            input.username,
            input.imap_host,
            input.imap_port,
            imap_security.as_str(),
            input.smtp_host,
            input.smtp_port,
            smtp_security.as_str(),
            color.clone(),
            input.signature,
            signature_fields,
            input.signature_on_new,
            id,
        ],
    )?;
    if changed == 0 {
        return Err(AppError::Message("account not found".into()));
    }
    if !input.password.trim().is_empty() {
        secrets.set(&account_key(id), &input.password)?;
    }
    let account = list(db)?
        .into_iter()
        .find(|account| account.id == id)
        .ok_or_else(|| AppError::Message("account not found".into()))?;
    if account.space_id == target_space_id {
        Ok(account)
    } else {
        move_to_space(db, id, target_space_id)
    }
}

pub fn set_color(db: &Database, id: i64, color: &str) -> AppResult<Account> {
    let color = account_color(color);
    let changed = db.conn.lock().unwrap().execute(
        "update accounts set color = ?1 where id = ?2",
        rusqlite::params![color, id],
    )?;
    if changed == 0 {
        return Err(AppError::Message("account not found".into()));
    }
    list(db)?
        .into_iter()
        .find(|account| account.id == id)
        .ok_or_else(|| AppError::Message("account not found".into()))
}

pub fn update_profile(
    db: &Database,
    id: i64,
    display_name: &str,
    color: &str,
    signature: &str,
    signature_fields: &SignatureFields,
    signature_on_new: bool,
) -> AppResult<Account> {
    let display_name = if display_name.trim().is_empty() {
        db.conn
            .lock()
            .unwrap()
            .query_row(
                "select email from accounts where id = ?1",
                rusqlite::params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| AppError::Message("account not found".into()))?
    } else {
        display_name.trim().to_string()
    };
    let color = account_color(color);
    let signature_fields = serde_json::to_string(signature_fields)?;
    let changed = db.conn.lock().unwrap().execute(
        "update accounts set display_name = ?1, color = ?2, signature = ?3, signature_fields = ?4, signature_on_new = ?5 where id = ?6",
        rusqlite::params![display_name, color.clone(), signature, signature_fields, signature_on_new, id],
    )?;
    if changed == 0 {
        return Err(AppError::Message("account not found".into()));
    }
    let account = list(db)?
        .into_iter()
        .find(|account| account.id == id)
        .ok_or_else(|| AppError::Message("account not found".into()))?;
    Ok(account)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountDiscovery {
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_security: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_security: String,
    pub source: String,
}

fn known_provider(domain: &str) -> Option<AccountDiscovery> {
    let (imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security) = match domain {
        "gmail.com" | "googlemail.com" => {
            ("imap.gmail.com", 993, "tls", "smtp.gmail.com", 465, "tls")
        }
        "outlook.com" | "hotmail.com" | "live.com" | "msn.com" => (
            "outlook.office365.com",
            993,
            "tls",
            "smtp.office365.com",
            587,
            "starttls",
        ),
        "yahoo.com" => (
            "imap.mail.yahoo.com",
            993,
            "tls",
            "smtp.mail.yahoo.com",
            465,
            "tls",
        ),
        "icloud.com" | "me.com" | "mac.com" => (
            "imap.mail.me.com",
            993,
            "tls",
            "smtp.mail.me.com",
            587,
            "starttls",
        ),
        "fastmail.com" => (
            "imap.fastmail.com",
            993,
            "tls",
            "smtp.fastmail.com",
            465,
            "tls",
        ),
        "gmx.com" | "gmx.net" => ("imap.gmx.com", 993, "tls", "smtp.gmx.com", 465, "tls"),
        _ => return None,
    };
    Some(AccountDiscovery {
        imap_host: imap_host.into(),
        imap_port,
        imap_security: imap_security.into(),
        smtp_host: smtp_host.into(),
        smtp_port,
        smtp_security: smtp_security.into(),
        source: "known".into(),
    })
}

fn server_blocks<'a>(xml: &'a str, kind: &str) -> Vec<&'a str> {
    let mut blocks = Vec::new();
    let open = format!("<{kind}");
    let close = format!("</{kind}>");
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start..];
        let Some(header_end) = after.find('>') else {
            break;
        };
        let header = &after[..header_end];
        let Some(end) = after.find(&close) else {
            break;
        };
        let block = &after[..end + close.len()];
        let expected = if kind == "incomingServer" {
            "imap"
        } else {
            "smtp"
        };
        if header
            .to_ascii_lowercase()
            .contains(&format!("type=\"{expected}\""))
        {
            blocks.push(block);
        }
        rest = &after[end + close.len()..];
    }
    blocks
}

fn tag_value(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find(&close)?;
    Some(block[start..start + end].trim().to_string())
}

fn security(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "ssl" | "tls" => "tls".into(),
        _ => "starttls".into(),
    }
}

fn parse_autoconfig(xml: &str) -> Option<AccountDiscovery> {
    let incoming = server_blocks(xml, "incomingServer").into_iter().next()?;
    let outgoing = server_blocks(xml, "outgoingServer").into_iter().next();
    let imap_host = tag_value(incoming, "hostname")?;
    let imap_port = tag_value(incoming, "port")?.parse().ok()?;
    let imap_security = security(&tag_value(incoming, "socketType").unwrap_or_default());
    let smtp_host = outgoing
        .and_then(|block| tag_value(block, "hostname"))
        .unwrap_or_else(|| format!("smtp.{imap_host}"));
    let smtp_port = outgoing
        .and_then(|block| tag_value(block, "port"))
        .and_then(|value| value.parse().ok())
        .unwrap_or(465);
    let smtp_security = outgoing
        .map(|block| security(&tag_value(block, "socketType").unwrap_or_default()))
        .unwrap_or_else(|| "tls".into());
    Some(AccountDiscovery {
        imap_host,
        imap_port,
        imap_security,
        smtp_host,
        smtp_port,
        smtp_security,
        source: "autoconfig".into(),
    })
}

async fn srv_target(
    resolver: &TokioAsyncResolver,
    name: &str,
    security: &str,
) -> Option<(String, u16, String)> {
    let lookup = tokio::time::timeout(
        std::time::Duration::from_secs(4),
        resolver.srv_lookup(name.to_string()),
    )
    .await
    .ok()?
    .ok()?;
    let record = lookup.iter().next()?;
    let host = record
        .target()
        .to_utf8()
        .trim_end_matches('.')
        .to_string();
    (!host.is_empty()).then(|| (host, record.port(), security.to_string()))
}

async fn discover_dns(domain: &str) -> Option<AccountDiscovery> {
    let resolver = TokioAsyncResolver::tokio_from_system_conf().ok()?;
    let imap = srv_target(&resolver, &format!("_imaps._tcp.{domain}"), "tls")
        .await
        .or(
            Box::pin(srv_target(
                &resolver,
                &format!("_imap._tcp.{domain}"),
                "starttls",
            ))
            .await,
        )?;
    let smtp = srv_target(
        &resolver,
        &format!("_submission._tcp.{domain}"),
        "starttls",
    )
    .await
    .or(
        Box::pin(srv_target(
            &resolver,
            &format!("_smtp._tcp.{domain}"),
            "starttls",
        ))
        .await,
    )?;
    Some(AccountDiscovery {
        imap_host: imap.0,
        imap_port: imap.1,
        imap_security: imap.2,
        smtp_host: smtp.0,
        smtp_port: smtp.1,
        smtp_security: smtp.2,
        source: "dns-srv".into(),
    })
}

async fn fetch_autoconfig(
    client: &reqwest::Client,
    url: reqwest::Url,
    source: &str,
) -> Option<AccountDiscovery> {
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let mut found = parse_autoconfig(&response.text().await.ok()?)?;
    found.source = source.to_string();
    Some(found)
}

pub async fn discover(email: &str) -> AppResult<AccountDiscovery> {
    let email = email.trim();
    let domain = email
        .rsplit_once('@')
        .map(|(_, domain)| domain.to_ascii_lowercase())
        .filter(|domain| !domain.is_empty())
        .ok_or_else(|| AppError::Message("Enter an email address first".into()))?;

    // Standard mail client discovery: DNS SRV records first, then the
    // domain-hosted Thunderbird autoconfig document.
    if let Some(found) = discover_dns(&domain).await {
        return Ok(found);
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .user_agent("AuraMail/0.1")
        .build()
        .map_err(|_| AppError::Message("Account discovery is unavailable".into()))?;
    for base in [
        format!("https://autoconfig.{domain}/mail/config-v1.1.xml"),
        format!("https://{domain}/.well-known/autoconfig/mail/config-v1.1.xml"),
    ] {
        if let Ok(mut url) = reqwest::Url::parse(&base) {
            url.query_pairs_mut().append_pair("emailaddress", email);
            if let Some(found) = fetch_autoconfig(&client, url, "autoconfig").await {
                return Ok(found);
            }
        }
    }
    if let Some(found) = fetch_autoconfig(
        &client,
        reqwest::Url::parse(&format!(
            "https://autoconfig.thunderbird.net/v1.1/{domain}"
        ))
        .map_err(|_| AppError::Message("Account discovery is unavailable".into()))?,
        "autoconfig-central",
    )
    .await
    {
        return Ok(found);
    }
    if let Some(provider) = known_provider(&domain) {
        return Ok(provider);
    }
    Ok(AccountDiscovery {
        imap_host: format!("imap.{domain}"),
        imap_port: 993,
        imap_security: "tls".into(),
        smtp_host: format!("smtp.{domain}"),
        smtp_port: 465,
        smtp_security: "tls".into(),
        source: "guess".into(),
    })
}

pub fn space_for_account(db: &Database, id: i64) -> AppResult<i64> {
    db.conn
        .lock()
        .unwrap()
        .query_row(
            "select space_id from accounts where id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| AppError::Message("account not found".into()))
}

pub fn list_for_space(db: &Database, space_id: i64) -> AppResult<Vec<Account>> {
    Ok(list(db)?
        .into_iter()
        .filter(|account| account.space_id == space_id)
        .collect())
}

pub fn reorder(db: &Database, space_id: i64, ids: &[i64]) -> AppResult<Vec<Account>> {
    let current = list_for_space(db, space_id)?;
    let current_ids = current.iter().map(|account| account.id).collect::<std::collections::HashSet<_>>();
    let requested_ids = ids.iter().copied().collect::<std::collections::HashSet<_>>();
    if current_ids != requested_ids || current_ids.len() != ids.len() {
        return Err(AppError::Message(
            "account order must include every account in the space exactly once".into(),
        ));
    }
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction()?;
    for (position, id) in ids.iter().enumerate() {
        tx.execute(
            "update accounts set position = ?1 where id = ?2 and space_id = ?3",
            rusqlite::params![position as i64, id, space_id],
        )?;
    }
    tx.commit()?;
    drop(conn);
    list_for_space(db, space_id)
}

pub fn move_to_space(db: &Database, id: i64, space_id: i64) -> AppResult<Account> {
    spaces::by_id(db, space_id)?;
    let mut conn = db.conn.lock().unwrap();
    let current_space: Option<i64> = conn
        .query_row(
            "select space_id from accounts where id = ?1",
            rusqlite::params![id],
            |row| row.get(0),
        )
        .optional()?;
    let current_space = current_space.ok_or_else(|| AppError::Message("account not found".into()))?;
    if current_space == space_id {
        drop(conn);
        return list(db)?
            .into_iter()
            .find(|account| account.id == id)
            .ok_or_else(|| AppError::Message("account not found".into()));
    }
    let position: i64 = conn.query_row(
        "select coalesce(max(position) + 1, 0) from accounts where space_id = ?1",
        rusqlite::params![space_id],
        |row| row.get(0),
    )?;
    let tx = conn.transaction()?;
    tx.execute(
        "update accounts set space_id = ?1, position = ?2 where id = ?3",
        rusqlite::params![space_id, position, id],
    )?;
    tx.execute(
        "delete from classifications where message_id in (select id from messages where account_id = ?1)",
        rusqlite::params![id],
    )?;
    tx.execute(
        "delete from ai_rail_cache where message_id in (select id from messages where account_id = ?1)",
        rusqlite::params![id],
    )?;
    tx.execute(
        "delete from message_embeddings where message_id in (select id from messages where account_id = ?1)",
        rusqlite::params![id],
    )?;
    tx.commit()?;
    drop(conn);
    list(db)?
        .into_iter()
        .find(|account| account.id == id)
        .ok_or_else(|| AppError::Message("account not found".into()))
}

pub fn list(db: &Database) -> AppResult<Vec<Account>> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn.prepare(
        "select id, display_name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, color, signature, signature_fields, signature_on_new, sync_interval_secs, created_at, auth_type, oauth_provider, space_id, position from accounts order by space_id, position, id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Account {
            id: row.get(0)?,
            display_name: row.get(1)?,
            email: row.get(2)?,
            username: row.get(3)?,
            imap_host: row.get(4)?,
            imap_port: row.get(5)?,
            imap_security: row.get(6)?,
            smtp_host: row.get(7)?,
            smtp_port: row.get(8)?,
            smtp_security: row.get(9)?,
            color: row.get(10)?,
            signature: row.get(11)?,
            signature_fields: serde_json::from_str(&row.get::<_, String>(12)?).unwrap_or_default(),
            signature_on_new: row.get(13)?,
            sync_interval_secs: row.get(14)?,
            created_at: row.get(15)?,
            auth_type: parse_auth_type(&row.get::<_, String>(16)?),
            oauth_provider: row.get(17)?,
            space_id: row.get(18)?,
            position: row.get(19)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn find_by_email_ci(db: &Database, email: &str) -> AppResult<Option<Account>> {
    let normalized = email.trim().to_ascii_lowercase();
    Ok(list(db)?
        .into_iter()
        .find(|account| account.email.trim().to_ascii_lowercase() == normalized))
}

/// OAuth identities are unique per provider, not globally by email. A user can
/// legitimately connect a Gmail account and a Microsoft account that both use
/// the same address. Manual accounts still reject any matching email.
pub fn oauth_identity_taken(db: &Database, email: &str, provider: &str) -> AppResult<bool> {
    let normalized = email.trim().to_ascii_lowercase();
    Ok(list(db)?.into_iter().any(|account| {
        account.email.trim().to_ascii_lowercase() == normalized
            && (account.auth_type != AccountAuth::OAuth
                || account.oauth_provider.trim().eq_ignore_ascii_case(provider.trim()))
    }))
}

pub fn delete(db: &Database, secrets: &dyn SecretStore, id: i64) -> AppResult<()> {
    let previous = secrets.get(&account_key(id)).ok();
    secrets.delete(&account_key(id))?;
    let deleted = db
        .conn
        .lock()
        .unwrap()
        .execute("delete from accounts where id = ?1", rusqlite::params![id]);
    match deleted {
        Ok(_) => Ok(()),
        Err(error) => {
            if let Some(password) = previous {
                let _ = secrets.set(&account_key(id), &password);
            }
            Err(error.into())
        }
    }
}

fn is_unique_violation(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts;
    use crate::db::Database;
    use crate::secrets::MemoryStore;
    use crate::spaces;

    fn input_in_space(space_id: i64) -> AccountInput {
        AccountInput {
            display_name: "Test".into(),
            email: "me@example.com".into(),
            username: "me@example.com".into(),
            password: "p@ss".into(),
            imap_host: "imap.example.com".into(),
            imap_port: 993,
            imap_security: "tls".into(),
            smtp_host: "smtp.example.com".into(),
            smtp_port: 465,
            smtp_security: "tls".into(),
            color: "accent".into(),
            signature: "— {name}".into(),
            signature_fields: SignatureFields::default(),
            signature_on_new: true,
            space_id,
        }
    }

    fn input() -> AccountInput {
        input_in_space(1)
    }

    fn oauth_input(email: &str) -> OAuthAccountInput {
        OAuthAccountInput {
            display_name: "Me".into(),
            email: email.into(),
            username: email.into(),
            imap_host: "imap.gmail.com".into(),
            imap_port: 993,
            imap_security: "tls".into(),
            smtp_host: "smtp.gmail.com".into(),
            smtp_port: 465,
            smtp_security: "tls".into(),
            color: "accent".into(),
            provider: "google".into(),
            space_id: 1,
        }
    }

    #[test]
    fn parses_autoconfig_and_known_providers() {
        let xml = r#"<clientConfig><emailProvider>
            <incomingServer type="imap"><hostname>imap.example.com</hostname><port>993</port><socketType>SSL</socketType></incomingServer>
            <outgoingServer type="smtp"><hostname>smtp.example.com</hostname><port>587</port><socketType>STARTTLS</socketType></outgoingServer>
        </emailProvider></clientConfig>"#;
        let found = super::parse_autoconfig(xml).unwrap();
        assert_eq!(found.imap_host, "imap.example.com");
        assert_eq!(found.imap_port, 993);
        assert_eq!(found.smtp_host, "smtp.example.com");
        assert_eq!(found.smtp_port, 587);
        assert_eq!(found.smtp_security, "starttls");
        assert_eq!(
            super::known_provider("gmail.com").unwrap().imap_host,
            "imap.gmail.com"
        );
    }

    #[test]
    fn account_secret_is_not_written_to_sqlite() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = accounts::create(&db, &secrets, input()).unwrap();
        assert_eq!(
            secrets.get(&format!("account:{}", account.id)).unwrap(),
            "p@ss"
        );
        let conn = db.conn.lock().unwrap();
        let found: i64 = conn
            .query_row(
                "select count(*) from accounts where email like '%p@ss%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(found, 0);
    }

    #[test]
    fn rejects_blank_fields_and_invalid_ports() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();

        let mut blank = input();
        blank.username = "  ".into();
        assert!(accounts::create(&db, &secrets, blank).is_err());

        let mut zero_port = input();
        zero_port.imap_port = 0;
        assert!(accounts::create(&db, &secrets, zero_port).is_err());

        let mut bad_security = input();
        bad_security.smtp_security = "plain".into();
        assert!(accounts::create(&db, &secrets, bad_security).is_err());

        assert!(accounts::list(&db).unwrap().is_empty());
    }

    #[test]
    fn update_changes_fields_and_keeps_the_saved_password() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = accounts::create(&db, &secrets, input()).unwrap();
        assert!(account.signature_on_new);

        let mut edit = input();
        edit.display_name = "Renamed".into();
        edit.username = "new@example.com".into();
        edit.email = "new@example.com".into();
        edit.color = "success".into();
        edit.signature_on_new = false;
        edit.signature_fields.profession = "Designer".into();
        edit.signature_fields.socials.push(SignatureSocial {
            label: "LinkedIn".into(),
            url: "https://linkedin.com/in/ada".into(),
        });
        edit.password = String::new();
        let updated = accounts::update(&db, &secrets, account.id, edit).unwrap();

        assert_eq!(updated.display_name, "Renamed");
        assert_eq!(updated.email, "new@example.com");
        assert_eq!(updated.color, "success");
        assert_eq!(updated.signature, "— {name}");
        assert!(!updated.signature_on_new);
        assert_eq!(updated.signature_fields.profession, "Designer");
        assert_eq!(updated.signature_fields.socials[0].label, "LinkedIn");
        assert_eq!(secrets.get(&account_key(account.id)).unwrap(), "p@ss");
    }

    #[test]
    fn list_and_delete_remove_account_and_secret() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = accounts::create(&db, &secrets, input()).unwrap();

        let listed = accounts::list(&db).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].email, "me@example.com");
        assert_eq!(listed[0].imap_port, 993);

        accounts::delete(&db, &secrets, account.id).unwrap();
        assert!(accounts::list(&db).unwrap().is_empty());
        assert!(secrets.get(&account_key(account.id)).is_err());
    }

    #[test]
    fn delete_is_retry_safe() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = accounts::create(&db, &secrets, input()).unwrap();

        accounts::delete(&db, &secrets, account.id).unwrap();
        accounts::delete(&db, &secrets, account.id).unwrap();
        assert!(accounts::list(&db).unwrap().is_empty());
        assert!(secrets.get(&account_key(account.id)).is_err());
    }

    #[test]
    fn delete_removes_an_orphaned_keyring_secret() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        secrets.set(&account_key(42), "orphan").unwrap();

        accounts::delete(&db, &secrets, 42).unwrap();

        assert!(secrets.get(&account_key(42)).is_err());
    }

    #[test]
    fn sync_interval_updates_every_account_and_new_accounts() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        assert_eq!(accounts::sync_interval(&db).unwrap(), 120);
        accounts::set_sync_interval(&db, 300).unwrap();
        assert_eq!(accounts::sync_interval(&db).unwrap(), 300);

        let account = accounts::create(&db, &secrets, input()).unwrap();
        assert_eq!(account.sync_interval_secs, 300);
        let updated = accounts::set_sync_interval(&db, 600).unwrap();
        assert_eq!(updated[0].sync_interval_secs, 600);
        assert!(accounts::set_sync_interval(&db, 30).is_err());
        assert!(accounts::set_sync_interval(&db, 7200).is_err());
        assert_eq!(
            accounts::fast_sync_interval(&db).unwrap(),
            accounts::DEFAULT_FAST_SYNC_INTERVAL_SECS
        );
        accounts::set_fast_sync_interval(&db, 60).unwrap();
        assert_eq!(accounts::fast_sync_interval(&db).unwrap(), 60);
        assert!(accounts::set_fast_sync_interval(&db, 5).is_err());
    }

    #[test]
    fn duplicate_email_returns_safe_error() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        accounts::create(&db, &secrets, input()).unwrap();

        let err = accounts::create(&db, &secrets, input()).unwrap_err();
        assert_eq!(err.to_string(), "account already exists");
        assert_eq!(accounts::list(&db).unwrap().len(), 1);
    }

    struct FailingStore;

    impl SecretStore for FailingStore {
        fn set(&self, _key: &str, _value: &str) -> AppResult<()> {
            Err(AppError::Message("secret store unavailable".into()))
        }
        fn get(&self, _key: &str) -> AppResult<String> {
            Err(AppError::Message("secret store unavailable".into()))
        }
        fn delete(&self, _key: &str) -> AppResult<()> {
            Err(AppError::Message("secret store unavailable".into()))
        }
    }

    #[test]
    fn removes_inserted_row_when_secret_storage_fails() {
        let db = Database::open_memory().unwrap();
        let result = accounts::create(&db, &FailingStore, input());
        assert!(result.is_err());
        assert!(accounts::list(&db).unwrap().is_empty());
    }

    #[test]
    fn new_accounts_default_to_password_auth() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        let account = accounts::create(&db, &secrets, input()).unwrap();
        assert_eq!(account.auth_type, AccountAuth::Password);
        assert_eq!(account.oauth_provider, "");
    }

    #[test]
    fn oauth_account_secret_is_not_written_to_sqlite() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        let input = oauth_input("me@gmail.com");
        let secret = r#"{"version":1,"provider":"google","refresh_token":"refresh","access_token":"access","expires_at":9999999999}"#;
        let account = accounts::create_oauth(&db, &secrets, input, secret).unwrap();
        assert_eq!(account.auth_type, AccountAuth::OAuth);
        assert_eq!(secrets.get(&account_key(account.id)).unwrap(), secret);
        let leaked: i64 = db.conn.lock().unwrap().query_row(
            "select count(*) from accounts where email like '%refresh%' or email like '%access%'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(leaked, 0);
    }

    #[test]
    fn duplicate_oauth_email_is_case_insensitive() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        accounts::create_oauth(&db, &secrets, oauth_input("Me@Gmail.com"), "{}").unwrap();
        let error = accounts::create_oauth(&db, &secrets, oauth_input("me@gmail.com"), "{}").unwrap_err();
        assert_eq!(error.to_string(), "This account is already added.");
    }

    #[test]
    fn oauth_allows_the_same_email_from_a_different_provider() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        accounts::create_oauth(&db, &secrets, oauth_input("me@gmail.com"), "{}").unwrap();

        let mut microsoft = oauth_input("ME@GMAIL.COM");
        microsoft.provider = "microsoft".into();
        microsoft.imap_host = "outlook.office365.com".into();
        microsoft.smtp_host = "smtp.office365.com".into();
        microsoft.smtp_port = 587;
        microsoft.smtp_security = "starttls".into();
        accounts::create_oauth(&db, &secrets, microsoft, "{}").unwrap();

        let accounts = accounts::list(&db).unwrap();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].email.to_ascii_lowercase(), "me@gmail.com");
        assert_eq!(accounts[1].email.to_ascii_lowercase(), "me@gmail.com");
        assert_ne!(accounts[0].oauth_provider, accounts[1].oauth_provider);
    }

    #[test]
    fn oauth_duplicate_detects_a_manual_gmail_account() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        let mut manual = input();
        manual.email = "me@gmail.com".into();
        manual.username = "me@gmail.com".into();
        accounts::create(&db, &secrets, manual).unwrap();
        let error = accounts::create_oauth(&db, &secrets, oauth_input("ME@GMAIL.COM"), "{}").unwrap_err();
        assert_eq!(error.to_string(), "This account is already added.");
    }

    #[test]
    fn accepts_custom_colors_and_defaults_an_empty_display_name_to_email() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        let mut custom = input();
        custom.color = "#12ABEF".into();
        custom.display_name = "  ".into();
        let account = accounts::create(&db, &secrets, custom).unwrap();
        assert_eq!(account.color, "#12abef");
        assert_eq!(account.display_name, "me@example.com");
        assert_eq!(account_color("not-a-color"), "blue");
    }

    #[test]
    fn moving_an_account_appends_it_to_the_target_space() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        let personal = spaces::ensure_default(&db).unwrap();
        let work = spaces::create(&db, "Work").unwrap();
        let first = accounts::create(&db, &secrets, input_in_space(personal.id)).unwrap();
        let mut second_input = input_in_space(personal.id);
        second_input.email = "second@example.com".into();
        second_input.username = "second@example.com".into();
        accounts::create(&db, &secrets, second_input).unwrap();

        let moved = accounts::move_to_space(&db, first.id, work.id).unwrap();
        assert_eq!(moved.space_id, work.id);
        assert_eq!(moved.position, 0);
        assert_eq!(accounts::list_for_space(&db, personal.id).unwrap().len(), 1);
    }

    #[test]
    fn moving_an_account_clears_old_space_ai_artifacts() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        let personal = spaces::ensure_default(&db).unwrap();
        let work = spaces::create(&db, "Work").unwrap();
        let account = accounts::create(&db, &secrets, input_in_space(personal.id)).unwrap();
        let (folder_id, message_id) = {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "insert into folders (account_id, path, name, role) values (?1, 'INBOX', 'Inbox', 'inbox')",
                rusqlite::params![account.id],
            )
            .unwrap();
            let folder_id = conn.last_insert_rowid();
            conn.execute(
                "insert into messages (account_id, folder_id, uid, subject) values (?1, ?2, 1, 'Hello')",
                rusqlite::params![account.id, folder_id],
            )
            .unwrap();
            (folder_id, conn.last_insert_rowid())
        };
        let tab_id: i64 = db
            .conn
            .lock()
            .unwrap()
            .query_row("select id from tabs order by position, id limit 1", [], |row| row.get(0))
            .unwrap();
        db.conn
            .lock()
            .unwrap()
            .execute_batch(&format!(
                "insert into classifications (message_id, tab_id, confidence, model, classified_at) values ({message_id}, {tab_id}, 1.0, 'test', 0);
                 insert into ai_rail_cache (message_id, model, full_body, thread_latest_id, thread_latest_date, payload, created_at) values ({message_id}, 'test', 0, {message_id}, 0, '{{}}', 0);
                 insert into message_embeddings (message_id, model, dim, vector, content_hash, created_at) values ({message_id}, 'test', 1, x'00', 'hash', 0);"
            ))
            .unwrap();

        accounts::move_to_space(&db, account.id, work.id).unwrap();

        let conn = db.conn.lock().unwrap();
        let counts: (i64, i64, i64, i64) = conn
            .query_row(
                "select
                    (select count(*) from classifications where message_id = ?1),
                    (select count(*) from ai_rail_cache where message_id = ?1),
                    (select count(*) from message_embeddings where message_id = ?1),
                    (select count(*) from messages where folder_id = ?2)",
                rusqlite::params![message_id, folder_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(counts, (0, 0, 0, 1));
    }

    #[test]
    fn reordering_accounts_requires_the_complete_space_set() {
        let db = Database::open_memory().unwrap();
        let secrets = MemoryStore::default();
        let personal = spaces::ensure_default(&db).unwrap();
        let first = accounts::create(&db, &secrets, input_in_space(personal.id)).unwrap();
        let mut second_input = input_in_space(personal.id);
        second_input.email = "second@example.com".into();
        second_input.username = "second@example.com".into();
        let second = accounts::create(&db, &secrets, second_input).unwrap();

        let reordered = accounts::reorder(&db, personal.id, &[second.id, first.id]).unwrap();
        assert_eq!(reordered.iter().map(|a| a.id).collect::<Vec<_>>(), vec![second.id, first.id]);
        assert_eq!(reordered[0].position, 0);
        assert_eq!(reordered[1].position, 1);
        assert!(accounts::reorder(&db, personal.id, &[second.id]).is_err());
    }
}
