//! Running rules: on newly arriving mail inside a sync, and over existing mail
//! for the manual sweep.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::AppHandle;

use crate::accounts::{self, Account};
use crate::db::Database;
use crate::error::AppResult;
use crate::imap::client::{self, ImapSession};
use crate::labels;
use crate::mail::{self, Folder, Message};
use crate::mail_auth::MailCredential;
use crate::rules::actions::{Engine, Target};
use crate::rules::eval::{self, Context, Facts, Verdict};
use crate::rules::{self, Priority, Rule, Sensitivity};

#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub matched: usize,
    pub moved: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

impl RunSummary {
    fn record(&mut self, errors: Vec<String>) {
        for error in errors {
            self.failed += 1;
            if self.errors.len() < 20 {
                self.errors.push(error);
            }
        }
    }

    pub fn merge(&mut self, other: RunSummary) {
        self.matched += other.matched;
        self.moved += other.moved;
        self.failed += other.failed;
        for error in other.errors {
            if self.errors.len() < 20 {
                self.errors.push(error);
            }
        }
    }
}

#[derive(Clone)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub summary: RunSummary,
}

struct People {
    addresses: Vec<String>,
    names: Vec<String>,
    domains: Vec<String>,
}

fn people(db: &Database, account: &Account) -> AppResult<People> {
    let all = accounts::list(db)?;
    let mut addresses = vec![account.email.clone()];
    let mut names = vec![account.display_name.clone(), account.identity.clone()];
    for other in all {
        if !other.email.trim().is_empty() {
            addresses.push(other.email);
        }
        if !other.display_name.trim().is_empty() {
            names.push(other.display_name);
        }
        if !other.identity.trim().is_empty() {
            names.push(other.identity);
        }
    }
    let mut domains = Vec::new();
    for address in &addresses {
        if let Some((_, domain)) = address.rsplit_once('@') {
            let domain = domain.trim().to_ascii_lowercase();
            if !domain.is_empty() && !domains.contains(&domain) {
                domains.push(domain);
            }
        }
    }
    Ok(People {
        addresses,
        names,
        domains,
    })
}

fn facts_for(db: &Database, message: &Message) -> AppResult<Facts> {
    let (importance, sensitivity): (String, String) = db.conn.lock().unwrap().query_row(
        "select importance, sensitivity from messages where id = ?1",
        [message.id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let labels = labels::for_message(
        db,
        message.account_id,
        &labels::message_key(message.message_id.as_deref(), message.id),
    )?;
    Ok(Facts {
        from_addr: message.from_addr.clone(),
        from_name: message.from_name.clone(),
        to: message.to_addrs.clone(),
        cc: message.cc_addrs.clone(),
        subject: message.subject.clone(),
        raw: None,
        body: None,
        labels,
        is_starred: message.is_starred,
        has_attachments: message.has_attachments,
        importance: Priority::parse(&importance),
        sensitivity: Sensitivity::parse(&sensitivity),
    })
}

/// Evaluate every enabled rule for one message and apply the matching actions.
/// Returns true when the message left its folder (moved or deleted).
fn run_message(
    engine: &Engine<'_>,
    target: &mut Target<'_>,
    rules: &[Rule],
    ctx: &Context<'_>,
    facts: &mut Facts,
) -> (bool, Vec<String>) {
    let mut errors = Vec::new();
    let mut fetched = facts.body.is_some();
    for rule in rules {
        let mut verdict = eval::evaluate(rule, facts, ctx);
        if verdict == Verdict::NeedMessage && !fetched {
            match client::fetch_message_in_session(target.session, target.uid) {
                Ok(raw) => {
                    eval::apply_raw(facts, &raw);
                    fetched = true;
                    verdict = eval::evaluate(rule, facts, ctx);
                }
                Err(error) => {
                    // Usually the size cap: the rule cannot judge this message,
                    // which must not abort the run.
                    errors.push(format!("{}: {error}", rule.name));
                    continue;
                }
            }
        }
        if verdict != Verdict::Yes {
            continue;
        }
        let mut left_folder = false;
        for action in &rule.actions {
            let outcome = engine.apply(target, action);
            if !outcome.errors.is_empty() {
                errors.extend(
                    outcome
                        .errors
                        .into_iter()
                        .map(|error| format!("{}: {error}", rule.name)),
                );
            }
            if outcome.moved || outcome.deleted {
                left_folder = true;
                break;
            }
        }
        if left_folder {
            return (true, errors);
        }
        // Actions are visible to later rules in the same run.
        if let Ok(refreshed) = facts_for(engine.db, &target.message) {
            facts.labels = refreshed.labels;
            facts.importance = refreshed.importance;
            facts.sensitivity = refreshed.sensitivity;
        }
        facts.is_starred = target.message.is_starred;
        if rule.stop_processing {
            break;
        }
    }
    (false, errors)
}

/// Apply rules to messages that just arrived. `raw_by_uid` carries the header
/// block the sync already fetched, so header conditions cost nothing.
#[allow(clippy::too_many_arguments)]
pub fn on_new_messages(
    app: &AppHandle,
    db: &Database,
    session: &mut ImapSession,
    account: &Account,
    credential: &MailCredential,
    folder: &Folder,
    new_ids: &[i64],
    raw_by_uid: &HashMap<u32, Vec<u8>>,
) -> RunSummary {
    let mut summary = RunSummary::default();
    if new_ids.is_empty() {
        return summary;
    }
    let rules = match rules::enabled_for_account(db, account.id) {
        Ok(rules) if !rules.is_empty() => rules,
        Ok(_) => return summary,
        Err(error) => {
            eprintln!("mail rules could not be loaded: {error}");
            return summary;
        }
    };
    let folders = match mail::list_folders(db, Some(account.id), None) {
        Ok(folders) => folders,
        Err(error) => {
            eprintln!("mail rules could not load folders: {error}");
            return summary;
        }
    };
    let people = match people(db, account) {
        Ok(people) => people,
        Err(error) => {
            eprintln!("mail rules could not resolve identities: {error}");
            return summary;
        }
    };
    let engine = Engine::new(
        app,
        db,
        account,
        credential,
        &folders,
        &people.addresses,
        true,
    );
    let ctx = Context {
        my_addresses: &people.addresses,
        my_names: &people.names,
        my_domains: &people.domains,
    };
    for id in new_ids {
        let Ok(message) = mail::message_by_id(db, *id) else {
            continue;
        };
        let uid = message.uid;
        let mut target = Target {
            session,
            folder: folder.clone(),
            uid,
            message,
        };
        let mut facts = match facts_for(db, &target.message) {
            Ok(facts) => facts,
            Err(error) => {
                summary.record(vec![error.to_string()]);
                continue;
            }
        };
        if let Some(raw) = raw_by_uid.get(&uid) {
            facts.raw = Some(raw.clone());
        }
        let (left, errors) = run_message(&engine, &mut target, &rules, &ctx, &mut facts);
        if !errors.is_empty() {
            summary.record(errors);
        }
        summary.matched += 1;
        if left {
            summary.moved += 1;
        }
    }
    summary
}

/// Manual sweep over one folder, or every Inbox when `folder_id` is None.
#[allow(clippy::too_many_arguments)]
pub fn sweep(
    app: &AppHandle,
    db: &Database,
    account: &Account,
    credential: &MailCredential,
    folder_id: Option<i64>,
    rule_id: Option<i64>,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> AppResult<RunSummary> {
    let all_folders = mail::list_folders(db, Some(account.id), None)?;
    let targets: Vec<Folder> = match folder_id {
        Some(id) => all_folders
            .iter()
            .filter(|folder| folder.id == id)
            .cloned()
            .collect(),
        None => all_folders
            .iter()
            .filter(|folder| folder.role == "inbox")
            .cloned()
            .collect(),
    };
    let mut rules = rules::enabled_for_account(db, account.id)?;
    if let Some(rule_id) = rule_id {
        rules.retain(|rule| rule.id == rule_id);
    }
    let mut summary = RunSummary::default();
    if rules.is_empty() || targets.is_empty() {
        return Ok(summary);
    }
    let people = people(db, account)?;
    let engine = Engine::new(
        app,
        db,
        account,
        credential,
        &all_folders,
        &people.addresses,
        false,
    );
    let ctx = Context {
        my_addresses: &people.addresses,
        my_names: &people.names,
        my_domains: &people.domains,
    };
    let total: usize = targets
        .iter()
        .map(|folder| mail::folder_message_count(db, folder.id).unwrap_or(0) as usize)
        .sum();
    let mut done = 0usize;
    let mut session = client::open_session(account, credential)?;
    for folder in &targets {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if let Err(error) = client::select_folder(&mut session, &folder.path) {
            summary.record(vec![error.to_string()]);
            continue;
        }
        let ids = match mail::folder_message_ids(db, folder.id) {
            Ok(ids) => ids,
            Err(error) => {
                summary.record(vec![error.to_string()]);
                continue;
            }
        };
        for id in ids {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            done += 1;
            let Ok(message) = mail::message_by_id(db, id) else {
                continue;
            };
            let mut target = Target {
                session: &mut session,
                folder: folder.clone(),
                uid: message.uid,
                message,
            };
            let mut facts = match facts_for(db, &target.message) {
                Ok(facts) => facts,
                Err(error) => {
                    summary.record(vec![error.to_string()]);
                    continue;
                }
            };
            let (left, errors) = run_message(&engine, &mut target, &rules, &ctx, &mut facts);
            if !errors.is_empty() {
                summary.record(errors);
            }
            summary.matched += 1;
            if left {
                summary.moved += 1;
            }
            progress(Progress {
                done,
                total,
                summary: summary.clone(),
            });
        }
    }
    let _ = session.logout();
    Ok(summary)
}
