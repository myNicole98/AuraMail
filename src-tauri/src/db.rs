use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension};

use crate::error::AppResult;

const SCHEMA: &str = r#"
create table accounts (
  id integer primary key autoincrement,
  display_name text not null,
  email text not null,
  username text not null,
  imap_host text not null,
  imap_port integer not null,
  imap_security text not null,
  smtp_host text not null,
  smtp_port integer not null,
  smtp_security text not null,
  sync_interval_secs integer not null default 120,
  color text not null default 'accent',
  signature text not null default '',
  signature_fields text not null default '{}',
  signature_on_new integer not null default 1,
  auth_type text not null default 'password',
  oauth_provider text not null default '',
  created_at integer not null
);

create table folders (
  id integer primary key autoincrement,
  account_id integer not null references accounts(id) on delete cascade,
  path text not null,
  name text not null,
  role text not null default 'other',
  delimiter text not null default '/',
  uid_validity integer not null default 0,
  highest_uid integer not null default 0,
  unique(account_id, path)
);

create table messages (
  id integer primary key autoincrement,
  account_id integer not null references accounts(id) on delete cascade,
  folder_id integer not null references folders(id) on delete cascade,
  uid integer not null,
  message_id text,
  references_header text,
  thread_key text not null default '',
  from_addr text not null default '',
  from_name text not null default '',
  to_addrs text not null default '[]',
  cc_addrs text not null default '[]',
  subject text not null default '',
  date integer not null default 0,
  snippet text not null default '',
  is_seen integer not null default 0,
  is_starred integer not null default 0,
  has_attachments integer not null default 0,
  body_cached integer not null default 0,
  unique(folder_id, uid)
);

create table message_bodies (
  message_id integer primary key references messages(id) on delete cascade,
  text_body text,
  html_body text,
  image_html text,
  fetched_at integer not null
);

create table attachments (
  id integer primary key autoincrement,
  message_id integer not null references messages(id) on delete cascade,
  filename text not null,
  mime_type text not null,
  size integer not null,
  cache_path text,
  content_id text
);

create table tabs (
  id integer primary key autoincrement,
  name text not null unique,
  description text not null,
  position integer not null,
  removable integer not null default 1,
  is_primary integer not null default 0,
  color text,
  icon text,
  created_at integer not null
);

create table classifications (
  message_id integer primary key references messages(id) on delete cascade,
  tab_id integer references tabs(id) on delete set null,
  confidence real not null default 0,
  model text not null,
  classified_at integer not null
);

create table ai_rail_cache (
  message_id integer primary key references messages(id) on delete cascade,
  model text not null,
  full_body integer not null,
  thread_latest_id integer not null,
  thread_latest_date integer not null,
  payload text not null,
  created_at integer not null
);

create index if not exists idx_ai_rail_cache_created on ai_rail_cache(created_at);

create table drafts (
  id integer primary key autoincrement,
  account_id integer references accounts(id) on delete cascade,
  in_reply_to text,
  references_header text,
  to_addrs text not null default '[]',
  cc_addrs text not null default '[]',
  subject text not null default '',
  body text not null default '',
  body_html text,
  inline_images text not null default '[]',
  attachments text not null default '[]',
  updated_at integer not null
);

create table settings (
  key text primary key,
  value text not null
);
"#;

pub(crate) const SEED_TABS: &[(&str, &str, bool, &str, &str)] = &[
    (
        "Primary",
        "General conversation, personal mail, and anything that does not fit another tab.",
        true,
        "#6da4ff",
        "inbox",
    ),
    (
        "Updates",
        "Receipts, confirmations, account notices, shipping, and automated status updates.",
        false,
        "#58c98b",
        "bell",
    ),
    (
        "Promotions",
        "Marketing, newsletters, offers, and commercial announcements.",
        false,
        "#f2b85b",
        "tag",
    ),
    (
        "Social",
        "Social network notifications, follows, mentions, and community activity.",
        false,
        "#e879b9",
        "users",
    ),
];

pub struct Database {
    pub(crate) conn: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::configure(&conn)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::configure(&conn)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn migrate(&self) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 1 {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(SCHEMA)?;
            let created_at = now();
            for (position, (name, description, primary, color, icon)) in
                SEED_TABS.iter().enumerate()
            {
                tx.execute(
                    "insert or ignore into tabs (name, description, position, is_primary, color, icon, created_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        name,
                        description,
                        position as i64,
                        *primary as i64,
                        color,
                        icon,
                        created_at
                    ],
                )?;
            }
            tx.pragma_update(None, "user_version", 1)?;
            tx.commit()?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 2 {
            let has_primary: i64 = conn.query_row(
                "select count(*) from pragma_table_info('tabs') where name = 'is_primary'",
                [],
                |row| row.get(0),
            )?;
            if has_primary == 0 {
                conn.execute(
                    "alter table tabs add column is_primary integer not null default 0",
                    [],
                )?;
            }
            conn.execute("update tabs set is_primary = 0", [])?;
            conn.execute(
                "update tabs set is_primary = 1 where id = coalesce((select id from tabs where lower(name) = 'primary' order by position, id limit 1), (select id from tabs order by position, id limit 1))",
                [],
            )?;
            conn.pragma_update(None, "user_version", 2)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 3 {
            let has_references: i64 = conn.query_row(
                "select count(*) from pragma_table_info('messages') where name = 'references_header'",
                [],
                |row| row.get(0),
            )?;
            if has_references == 0 {
                conn.execute("alter table messages add column references_header text", [])?;
            }
            conn.pragma_update(None, "user_version", 3)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 4 {
            conn.execute_batch("delete from message_bodies; update messages set body_cached = 0;")?;
            conn.pragma_update(None, "user_version", 4)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 5 {
            conn.execute_batch("delete from message_bodies; update messages set body_cached = 0;")?;
            conn.pragma_update(None, "user_version", 5)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 6 {
            conn.execute_batch("delete from message_bodies; update messages set body_cached = 0;")?;
            conn.pragma_update(None, "user_version", 6)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 7 {
            conn.execute_batch("delete from message_bodies; update messages set body_cached = 0;")?;
            conn.pragma_update(None, "user_version", 7)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 8 {
            let has_image_html: i64 = conn.query_row(
                "select count(*) from pragma_table_info('message_bodies') where name = 'image_html'",
                [],
                |row| row.get(0),
            )?;
            if has_image_html == 0 {
                conn.execute("alter table message_bodies add column image_html text", [])?;
            }
            conn.pragma_update(None, "user_version", 8)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 9 {
            let has_color: i64 = conn.query_row(
                "select count(*) from pragma_table_info('accounts') where name = 'color'",
                [],
                |row| row.get(0),
            )?;
            if has_color == 0 {
                conn.execute(
                    "alter table accounts add column color text not null default 'accent'",
                    [],
                )?;
            }
            conn.execute_batch(
                "update accounts set color = case id % 6
                    when 0 then 'accent'
                    when 1 then 'accent-alt'
                    when 2 then 'success'
                    when 3 then 'warning'
                    when 4 then 'info'
                    else 'danger' end;",
            )?;
            conn.pragma_update(None, "user_version", 9)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 10 {
            conn.execute("update folders set uid_validity = 0, highest_uid = 0", [])?;
            conn.pragma_update(None, "user_version", 10)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 11 {
            let has_html: i64 = conn.query_row(
                "select count(*) from pragma_table_info('drafts') where name = 'body_html'",
                [],
                |row| row.get(0),
            )?;
            if has_html == 0 {
                conn.execute("alter table drafts add column body_html text", [])?;
            }
            let has_inline: i64 = conn.query_row(
                "select count(*) from pragma_table_info('drafts') where name = 'inline_images'",
                [],
                |row| row.get(0),
            )?;
            if has_inline == 0 {
                conn.execute(
                    "alter table drafts add column inline_images text not null default '[]'",
                    [],
                )?;
            }
            conn.pragma_update(None, "user_version", 11)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 12 {
            let has_signature: i64 = conn.query_row(
                "select count(*) from pragma_table_info('accounts') where name = 'signature'",
                [],
                |row| row.get(0),
            )?;
            if has_signature == 0 {
                conn.execute(
                    "alter table accounts add column signature text not null default ''",
                    [],
                )?;
            }
            conn.pragma_update(None, "user_version", 12)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 13 {
            let has_fields: i64 = conn.query_row(
                "select count(*) from pragma_table_info('accounts') where name = 'signature_fields'",
                [],
                |row| row.get(0),
            )?;
            if has_fields == 0 {
                conn.execute(
                    "alter table accounts add column signature_fields text not null default '{}'",
                    [],
                )?;
            }
            conn.pragma_update(None, "user_version", 13)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 14 {
            let has_thread: i64 = conn.query_row(
                "select count(*) from pragma_table_info('messages') where name = 'thread_key'",
                [],
                |row| row.get(0),
            )?;
            if has_thread == 0 {
                conn.execute(
                    "alter table messages add column thread_key text not null default ''",
                    [],
                )?;
            }
            conn.execute(
                "update messages set thread_key = 'local:' || id where thread_key = ''",
                [],
            )?;
            let rows = {
                let mut stmt = conn.prepare(
                    "select id, message_id, references_header from messages where thread_key like 'local:%'",
                )?;
                let mapped = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })?;
                mapped.collect::<Result<Vec<_>, _>>()?
            };
            for (id, message_id, references) in rows {
                if let Some(key) = message_thread_key(message_id.as_deref(), references.as_deref())
                {
                    conn.execute(
                        "update messages set thread_key = ?1 where id = ?2",
                        rusqlite::params![key, id],
                    )?;
                }
            }
            conn.pragma_update(None, "user_version", 14)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 15 {
            let has_color: i64 = conn.query_row(
                "select count(*) from pragma_table_info('tabs') where name = 'color'",
                [],
                |row| row.get(0),
            )?;
            if has_color == 0 {
                conn.execute("alter table tabs add column color text", [])?;
            }
            let has_icon: i64 = conn.query_row(
                "select count(*) from pragma_table_info('tabs') where name = 'icon'",
                [],
                |row| row.get(0),
            )?;
            if has_icon == 0 {
                conn.execute("alter table tabs add column icon text", [])?;
            }
            conn.execute_batch(
                "update tabs set color = case lower(name)
                    when 'primary' then '#6da4ff'
                    when 'updates' then '#58c98b'
                    when 'promotions' then '#f2b85b'
                    when 'social' then '#e879b9'
                    else color end
                 where color is null;
                 update tabs set icon = case lower(name)
                    when 'primary' then 'inbox'
                    when 'updates' then 'bell'
                    when 'promotions' then 'tag'
                    when 'social' then 'users'
                    else icon end
                 where icon is null;
                 update tabs set color = case position % 6
                    when 0 then '#6da4ff'
                    when 1 then '#58c98b'
                    when 2 then '#f2b85b'
                    when 3 then '#e879b9'
                    when 4 then '#b489ff'
                    else '#f28f6b' end
                 where color is null;
                 update tabs set icon = 'folder' where icon is null;",
            )?;
            conn.pragma_update(None, "user_version", 15)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 16 {
            conn.execute_batch(
                "create table if not exists ai_rail_cache (
                    message_id integer primary key references messages(id) on delete cascade,
                    model text not null,
                    full_body integer not null,
                    thread_latest_id integer not null,
                    thread_latest_date integer not null,
                    payload text not null,
                    created_at integer not null
                );
                create index if not exists idx_ai_rail_cache_created on ai_rail_cache(created_at);",
            )?;
            conn.pragma_update(None, "user_version", 16)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 17 {
            conn.execute("delete from ai_rail_cache", [])?;
            conn.pragma_update(None, "user_version", 17)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 18 {
            conn.execute_batch(
                "create table if not exists message_embeddings (
                    message_id integer primary key references messages(id) on delete cascade,
                    model text not null,
                    dim integer not null,
                    vector blob not null,
                    content_hash text not null,
                    created_at integer not null
                );
                create index if not exists idx_message_embeddings_model on message_embeddings(model);",
            )?;
            conn.pragma_update(None, "user_version", 18)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 19 {
            conn.execute_batch(
                "create table if not exists sender_avatars (
                    email text primary key,
                    source text not null,
                    mime text,
                    bytes blob,
                    fetched_at integer not null
                );",
            )?;
            conn.pragma_update(None, "user_version", 19)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 20 {
            let has_auth_type: i64 = conn.query_row(
                "select count(*) from pragma_table_info('accounts') where name = 'auth_type'",
                [],
                |row| row.get(0),
            )?;
            if has_auth_type == 0 {
                conn.execute(
                    "alter table accounts add column auth_type text not null default 'password'",
                    [],
                )?;
            }
            let has_provider: i64 = conn.query_row(
                "select count(*) from pragma_table_info('accounts') where name = 'oauth_provider'",
                [],
                |row| row.get(0),
            )?;
            if has_provider == 0 {
                conn.execute(
                    "alter table accounts add column oauth_provider text not null default ''",
                    [],
                )?;
            }
            conn.pragma_update(None, "user_version", 20)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 21 {
            let has_tab_space: i64 = conn.query_row(
                "select count(*) from pragma_table_info('tabs') where name = 'space_id'",
                [],
                |row| row.get(0),
            )?;
            let has_account_space: i64 = conn.query_row(
                "select count(*) from pragma_table_info('accounts') where name = 'space_id'",
                [],
                |row| row.get(0),
            )?;
            if has_tab_space > 0 && has_account_space > 0 {
                // Simulated older schemas in tests may reset user_version after
                // the structural migration already ran.
                conn.pragma_update(None, "user_version", 21)?;
            } else {
                conn.execute_batch("pragma foreign_keys = off;")?;
                let tx = conn.unchecked_transaction()?;
                tx.execute_batch(
                    "create table if not exists spaces (
                        id integer primary key autoincrement,
                        name text not null collate nocase unique,
                        position integer not null,
                        created_at integer not null
                    );
                    drop table if exists tabs_new;
                    create table tabs_new (
                        id integer primary key autoincrement,
                        space_id integer not null references spaces(id) on delete restrict,
                        name text not null,
                        description text not null,
                        position integer not null,
                        removable integer not null default 1,
                        is_primary integer not null default 0,
                        color text,
                        icon text,
                        created_at integer not null,
                        unique(space_id, name)
                    );",
                )?;
                tx.execute(
                    "insert or ignore into spaces (name, position, created_at) values ('Personal', 0, ?1)",
                    rusqlite::params![now()],
                )?;
                let default_space: i64 = tx.query_row(
                    "select id from spaces where lower(name) = 'personal'",
                    [],
                    |row| row.get(0),
                )?;
                tx.execute(
                    "insert into tabs_new (id, space_id, name, description, position, removable, is_primary, color, icon, created_at)
                     select id, ?1, name, description, position, removable, is_primary, color, icon, created_at from tabs",
                    rusqlite::params![default_space],
                )?;
                tx.execute_batch(
                    "drop table tabs;
                     alter table tabs_new rename to tabs;",
                )?;
                if has_account_space == 0 {
                    tx.execute(
                        "alter table accounts add column space_id integer references spaces(id) on delete restrict",
                        [],
                    )?;
                }
                let has_account_position: i64 = tx.query_row(
                    "select count(*) from pragma_table_info('accounts') where name = 'position'",
                    [],
                    |row| row.get(0),
                )?;
                if has_account_position == 0 {
                    tx.execute(
                        "alter table accounts add column position integer not null default 0",
                        [],
                    )?;
                }
                tx.execute(
                    "update accounts set space_id = ?1",
                    rusqlite::params![default_space],
                )?;
                tx.execute(
                    "update accounts set position = (
                        select count(*) from accounts earlier
                        where earlier.space_id = accounts.space_id and earlier.id < accounts.id
                     )",
                    [],
                )?;
                tx.pragma_update(None, "user_version", 21)?;
                tx.commit()?;
                conn.execute_batch("pragma foreign_keys = on;")?;
            }
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 22 {
            let default_space: Option<i64> = conn
                .query_row(
                    "select id from spaces where lower(name) in ('default', 'personal') order by case lower(name) when 'default' then 0 else 1 end limit 1",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(space_id) = default_space {
                for key in [
                    "theme.choice",
                    "theme.accent",
                    "theme.background",
                    "theme.panes",
                ] {
                    conn.execute(
                        "insert or ignore into settings (key, value) select ?1, value from settings where key = ?2",
                        rusqlite::params![format!("space:{space_id}:{key}"), key],
                    )?;
                }
            }
            conn.pragma_update(None, "user_version", 22)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 23 {
            let default_space: Option<i64> = conn
                .query_row(
                    "select id from spaces where lower(name) in ('default', 'personal') order by case lower(name) when 'default' then 0 else 1 end limit 1",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(space_id) = default_space {
                for key in [
                    "ai.config",
                    "ai.auto_classify",
                    "ai.auto_rail",
                    "ai.full_body_context",
                    "ai.match_voice",
                    "ai.key_url",
                    "ai.voice_profile",
                    "ai.voice_last_attempt",
                ] {
                    conn.execute(
                        "insert or ignore into settings (key, value) select ?1, value from settings where key = ?2",
                        rusqlite::params![format!("space:{space_id}:{key}"), key],
                    )?;
                }
            }
            conn.pragma_update(None, "user_version", 23)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 24 {
            let has_icon: i64 = conn.query_row(
                "select count(*) from pragma_table_info('spaces') where name = 'icon'",
                [],
                |row| row.get(0),
            )?;
            if has_icon == 0 {
                conn.execute(
                    "alter table spaces add column icon text not null default 'layers'",
                    [],
                )?;
            }
            conn.pragma_update(None, "user_version", 24)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 25 {
            let spaces: Vec<i64> = {
                let mut stmt = conn.prepare("select id from spaces")?;
                let rows = stmt.query_map([], |row| row.get(0))?;
                rows.collect::<Result<_, _>>()?
            };
            for space_id in spaces {
                for key in [
                    "mail.trusted_senders",
                    "mail.always_load_images",
                    "mail.profile_pictures",
                ] {
                    conn.execute(
                        "insert or ignore into settings (key, value) select ?1, value from settings where key = ?2",
                        rusqlite::params![format!("space:{space_id}:{key}"), key],
                    )?;
                }
            }
            conn.pragma_update(None, "user_version", 25)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 26 {
            let has_signature_on_new: i64 = conn.query_row(
                "select count(*) from pragma_table_info('accounts') where name = 'signature_on_new'",
                [],
                |row| row.get(0),
            )?;
            if has_signature_on_new == 0 {
                conn.execute(
                    "alter table accounts add column signature_on_new integer not null default 1",
                    [],
                )?;
            }
            conn.pragma_update(None, "user_version", 26)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 27 {
            conn.execute(
                "update spaces set name = 'Personal'
                 where lower(name) = 'default'
                   and not exists (
                     select 1 from spaces personal
                     where personal.id <> spaces.id
                       and lower(personal.name) = 'personal'
                   )",
                [],
            )?;
            conn.execute(
                "update spaces set icon = 'user'
                 where lower(name) = 'personal' and icon = 'layers'",
                [],
            )?;
            conn.pragma_update(None, "user_version", 27)?;
        }
        let version: i64 = conn.query_row("pragma user_version", [], |row| row.get(0))?;
        if version < 28 {
            // Older databases have a globally unique email. Google and
            // Microsoft can legitimately point at the same address, so rebuild
            // the table without that constraint and enforce identity matching
            // in the account creation paths instead.
            conn.execute_batch("pragma foreign_keys = off;")?;
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(
                "drop table if exists accounts_new;
                 create table accounts_new (
                   id integer primary key autoincrement,
                   display_name text not null,
                   email text not null,
                   username text not null,
                   imap_host text not null,
                   imap_port integer not null,
                   imap_security text not null,
                   smtp_host text not null,
                   smtp_port integer not null,
                   smtp_security text not null,
                   sync_interval_secs integer not null default 120,
                   color text not null default 'accent',
                   signature text not null default '',
                   signature_fields text not null default '{}',
                   signature_on_new integer not null default 1,
                   auth_type text not null default 'password',
                   oauth_provider text not null default '',
                   created_at integer not null,
                   space_id integer references spaces(id) on delete restrict,
                   position integer not null default 0
                 );
                 insert into accounts_new (
                   id, display_name, email, username, imap_host, imap_port,
                   imap_security, smtp_host, smtp_port, smtp_security,
                   sync_interval_secs, color, signature, signature_fields,
                   signature_on_new, auth_type, oauth_provider, created_at,
                   space_id, position
                 )
                 select
                   id, display_name, email, username, imap_host, imap_port,
                   imap_security, smtp_host, smtp_port, smtp_security,
                   sync_interval_secs, color, signature, signature_fields,
                   signature_on_new, auth_type, oauth_provider, created_at,
                   space_id, position
                 from accounts;
                 drop table accounts;
                 alter table accounts_new rename to accounts;",
            )?;
            tx.pragma_update(None, "user_version", 28)?;
            tx.commit()?;
            conn.execute_batch("pragma foreign_keys = on;")?;
        }
        conn.execute_batch(
            "create index if not exists idx_messages_folder_date on messages(folder_id, date desc);
             create index if not exists idx_messages_account_date on messages(account_id, date desc);
             create index if not exists idx_messages_account_message_id on messages(account_id, message_id);
             create index if not exists idx_folders_account_role on folders(account_id, role);
             create index if not exists idx_messages_thread on messages(account_id, thread_key, date desc);
             create index if not exists idx_attachments_message on attachments(message_id);
             create index if not exists idx_drafts_account_updated on drafts(account_id, updated_at desc);
             create index if not exists idx_accounts_email on accounts(email);
             create index if not exists idx_accounts_space_position on accounts(space_id, position);
             create index if not exists idx_tabs_space_position on tabs(space_id, position);",
        )?;
        Ok(())
    }

    fn configure(conn: &Connection) -> AppResult<()> {
        conn.execute_batch("pragma foreign_keys=on; pragma journal_mode=wal;")?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> AppResult<Option<String>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                "select value from settings where key = ?1",
                rusqlite::params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "insert into settings (key, value) values (?1, ?2) on conflict(key) do update set value = excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    pub fn delete_setting(&self, key: &str) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "delete from settings where key = ?1",
            rusqlite::params![key],
        )?;
        Ok(())
    }
}

pub fn message_thread_key(message_id: Option<&str>, references: Option<&str>) -> Option<String> {
    let root = references
        .and_then(|value| value.split_whitespace().next())
        .or(message_id)?;
    let normalized = root.trim().trim_matches('<').trim_matches('>').trim();
    (!normalized.is_empty()).then(|| normalized.to_ascii_lowercase())
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_all_tables_and_seeds_tabs() {
        let db = Database::open_memory().unwrap();
        db.migrate().unwrap();
        let conn = db.conn.lock().unwrap();
        let tables: Vec<String> = conn
            .prepare("select name from sqlite_master where type='table' order by name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for name in [
            "spaces",
            "accounts",
            "folders",
            "messages",
            "message_bodies",
            "attachments",
            "tabs",
            "classifications",
            "drafts",
            "settings",
            "ai_rail_cache",
            "message_embeddings",
        ] {
            assert!(tables.contains(&name.to_string()), "missing {name}");
        }
        let tabs: i64 = conn
            .query_row("select count(*) from tabs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(tabs, 4);
    }

    #[test]
    fn open_creates_parent_dirs_and_migrates_idempotently() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("auramail.db");
        let db = Database::open(&path).unwrap();
        db.migrate().unwrap();
        assert!(path.exists());

        let conn = db.conn.lock().unwrap();
        let version: i64 = conn
            .query_row("pragma user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 28);
        let tabs: i64 = conn
            .query_row("select count(*) from tabs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(tabs, 4);
        let primary: i64 = conn
            .query_row(
                "select count(*) from tabs where is_primary = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(primary, 1);
        let tab_color: Option<String> = conn
            .query_row("select color from tabs where name = 'Primary'", [], |row| {
                row.get(0)
            })
            .unwrap();
        let tab_icon: Option<String> = conn
            .query_row("select icon from tabs where name = 'Primary'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(tab_color.is_some());
        assert!(tab_icon.is_some());
        let journal: String = conn
            .query_row("pragma journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal, "wal");
        let foreign_keys: i64 = conn
            .query_row("pragma foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);
    }

    #[test]
    fn migrates_tab_appearance_columns_and_defaults() {
        let db = Database::open_memory().unwrap();
        let conn = db.conn.lock().unwrap();
        let color_column: i64 = conn
            .query_row(
                "select count(*) from pragma_table_info('tabs') where name = 'color'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let icon_column: i64 = conn
            .query_row(
                "select count(*) from pragma_table_info('tabs') where name = 'icon'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(color_column, 1);
        assert_eq!(icon_column, 1);
        let missing: i64 = conn
            .query_row(
                "select count(*) from tabs where color is null or icon is null",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(missing, 0);
    }

    #[test]
    fn migration_invalidates_cached_mail_rendering() {
        let db = Database::open_memory().unwrap();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute_batch(
                "insert into accounts (id, display_name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, sync_interval_secs, created_at) values (1, 'A', 'a@example.com', 'a', 'imap', 993, 'tls', 'smtp', 465, 'tls', 120, 0);
                 insert into folders (id, account_id, path, name, role) values (1, 1, 'INBOX', 'Inbox', 'inbox');
                 insert into messages (id, account_id, folder_id, uid, body_cached) values (1, 1, 1, 1, 1);
                 insert into message_bodies (message_id, text_body, html_body, fetched_at) values (1, 'old', '<p>old</p>', 0);
                 pragma user_version = 3;",
            )
            .unwrap();
        }
        db.migrate().unwrap();
        let conn = db.conn.lock().unwrap();
        let bodies: i64 = conn
            .query_row("select count(*) from message_bodies", [], |row| row.get(0))
            .unwrap();
        let cached: i64 = conn
            .query_row("select body_cached from messages where id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        let version: i64 = conn
            .query_row("pragma user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(bodies, 0);
        assert_eq!(cached, 0);
        assert_eq!(version, 28);
    }

    #[test]
    fn migration_adds_the_sender_avatar_cache() {
        let db = Database::open_memory().unwrap();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute_batch(
                "drop table sender_avatars;
                 pragma user_version = 18;",
            )
            .unwrap();
        }
        db.migrate().unwrap();
        let conn = db.conn.lock().unwrap();
        let table: i64 = conn
            .query_row(
                "select count(*) from sqlite_master where type = 'table' and name = 'sender_avatars'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let version: i64 = conn
            .query_row("pragma user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(table, 1);
        assert_eq!(version, 28);
    }

    #[test]
    fn migration_clears_the_previous_rail_cache_format() {
        let db = Database::open_memory().unwrap();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute_batch(
                "insert into accounts (id, display_name, email, username, imap_host, imap_port, imap_security, smtp_host, smtp_port, smtp_security, sync_interval_secs, created_at) values (1, 'A', 'a@example.com', 'a', 'imap', 993, 'tls', 'smtp', 465, 'tls', 120, 0);
                 insert into folders (id, account_id, path, name, role) values (1, 1, 'INBOX', 'Inbox', 'inbox');
                 insert into messages (id, account_id, folder_id, uid) values (1, 1, 1, 1);
                 insert into ai_rail_cache (message_id, model, full_body, thread_latest_id, thread_latest_date, payload, created_at)
                 values (1, 'model-a', 0, 1, 100, '{}', 0);
                 pragma user_version = 16;",
            )
            .unwrap();
        }
        db.migrate().unwrap();
        let conn = db.conn.lock().unwrap();
        let cached: i64 = conn
            .query_row("select count(*) from ai_rail_cache", [], |row| row.get(0))
            .unwrap();
        let version: i64 = conn
            .query_row("pragma user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(cached, 0);
        assert_eq!(version, 28);
    }

    #[test]
    fn migration_creates_default_space_and_scopes_accounts_and_tabs() {
        let db = Database::open_memory().unwrap();
        let conn = db.conn.lock().unwrap();
        let spaces: i64 = conn
            .query_row("select count(*) from spaces", [], |row| row.get(0))
            .unwrap();
        assert_eq!(spaces, 1);
        let default_space: String = conn
            .query_row(
                "select name from spaces order by position, id limit 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(default_space, "Personal");
        let unscoped_accounts: i64 = conn
            .query_row(
                "select count(*) from accounts where space_id is null",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let unscoped_tabs: i64 = conn
            .query_row(
                "select count(*) from tabs where space_id is null",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((unscoped_accounts, unscoped_tabs), (0, 0));
        let icon: String = conn
            .query_row("select icon from spaces order by position, id limit 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(icon, "user");
        let version: i64 = conn
            .query_row("pragma user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 28);
    }

    #[test]
    fn migration_renames_the_legacy_default_space_to_personal() {
        let db = Database::open_memory().unwrap();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute_batch(
                "update spaces set name = 'Default', icon = 'layers';
                 pragma user_version = 26;",
            )
            .unwrap();
        }
        db.migrate().unwrap();
        let conn = db.conn.lock().unwrap();
        let (name, icon): (String, String) = conn
            .query_row("select name, icon from spaces order by position, id limit 1", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!((name.as_str(), icon.as_str()), ("Personal", "user"));
    }

    #[test]
    fn now_returns_recent_unix_seconds() {
        let value = now();
        assert!(value > 1_700_000_000);
        assert!(value <= super::now());
    }

    #[test]
    fn settings_round_trip_update_and_delete() {
        let db = Database::open_memory().unwrap();
        assert_eq!(db.setting("ai.config").unwrap(), None);
        db.set_setting("ai.config", "{\"model\":\"one\"}").unwrap();
        assert_eq!(
            db.setting("ai.config").unwrap().as_deref(),
            Some("{\"model\":\"one\"}")
        );
        db.set_setting("ai.config", "{\"model\":\"two\"}").unwrap();
        assert_eq!(
            db.setting("ai.config").unwrap().as_deref(),
            Some("{\"model\":\"two\"}")
        );
        db.delete_setting("ai.config").unwrap();
        assert_eq!(db.setting("ai.config").unwrap(), None);
        db.delete_setting("ai.config").unwrap();
    }

    #[test]
    fn migration_adds_account_auth_columns() {
        let db = Database::open_memory().unwrap();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute_batch(
                "alter table accounts drop column auth_type;
                 alter table accounts drop column oauth_provider;
                 pragma user_version = 19;",
            )
            .unwrap();
        }
        db.migrate().unwrap();
        let conn = db.conn.lock().unwrap();
        let auth_type: i64 = conn
            .query_row(
                "select count(*) from pragma_table_info('accounts') where name = 'auth_type'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let provider: i64 = conn
            .query_row(
                "select count(*) from pragma_table_info('accounts') where name = 'oauth_provider'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let version: i64 = conn
            .query_row("pragma user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!((auth_type, provider, version), (1, 1, 28));
    }
}
