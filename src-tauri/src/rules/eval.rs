//! Pure rule evaluation. No DB, no IMAP: the engine supplies `Facts`, fetches
//! the full message when the evaluator asks for it, and retries.

use crate::mail::parse;
use crate::rules::{Condition, MessageType, Rule};

/// What the evaluator knows about a message before and after the full body is
/// fetched.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub from_addr: String,
    pub from_name: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    /// Raw message bytes once fetched; `None` for header-only facts.
    pub raw: Option<Vec<u8>>,
    /// Plain-text body once fetched.
    pub body: Option<String>,
    pub labels: Vec<i64>,
    pub is_starred: bool,
    pub has_attachments: bool,
    pub importance: crate::rules::Priority,
    pub sensitivity: crate::rules::Sensitivity,
}

pub struct Context<'a> {
    pub my_addresses: &'a [String],
    pub my_names: &'a [String],
    pub my_domains: &'a [String],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Yes,
    No,
    /// A condition needs the full message; the engine fetches it and re-runs.
    NeedMessage,
}

pub fn evaluate(rule: &Rule, facts: &Facts, ctx: &Context) -> Verdict {
    for condition in &rule.conditions {
        match check(condition, facts, ctx) {
            Verdict::No => return Verdict::No,
            Verdict::NeedMessage => return Verdict::NeedMessage,
            Verdict::Yes => {}
        }
    }
    for condition in &rule.exceptions {
        match check(condition, facts, ctx) {
            Verdict::Yes => return Verdict::No,
            Verdict::NeedMessage => return Verdict::NeedMessage,
            Verdict::No => {}
        }
    }
    Verdict::Yes
}

fn yes(condition: bool) -> Verdict {
    if condition {
        Verdict::Yes
    } else {
        Verdict::No
    }
}

fn contains(haystack: &str, needle: &str) -> bool {
    let needle = needle.trim();
    if needle.is_empty() {
        return false;
    }
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn any_contains(haystack: &[String], needle: &str) -> bool {
    haystack.iter().any(|value| contains(value, needle))
}

fn is_me(value: &str, ctx: &Context) -> bool {
    let value = value.to_lowercase();
    ctx.my_addresses
        .iter()
        .any(|address| !address.trim().is_empty() && value.contains(&address.to_lowercase()))
}

fn mentions_me(text: &str, ctx: &Context) -> bool {
    ctx.my_addresses
        .iter()
        .chain(ctx.my_names.iter())
        .any(|needle| contains(text, needle))
}

fn domain_of(address: &str) -> Option<&str> {
    let address = address.trim();
    let at = address.rfind('@')?;
    Some(&address[at + 1..])
}

fn any_value(values: &[String], matches: impl Fn(&str) -> bool) -> bool {
    values.iter().any(|value| matches(value))
}

fn check(condition: &Condition, facts: &Facts, ctx: &Context) -> Verdict {
    match condition {
        Condition::FromContains { values } => yes(any_value(values, |value| {
            contains(&facts.from_addr, value) || contains(&facts.from_name, value)
        })),
        Condition::ToContains { values } => {
            yes(any_value(values, |value| any_contains(&facts.to, value)))
        }
        Condition::CcContains { values } => {
            yes(any_value(values, |value| any_contains(&facts.cc, value)))
        }
        Condition::SenderAddressContains { values } => {
            yes(any_value(values, |value| contains(&facts.from_addr, value)))
        }
        Condition::RecipientAddressContains { values } => yes(any_value(values, |value| {
            any_contains(&facts.to, value) || any_contains(&facts.cc, value)
        })),
        Condition::SubjectContains { values } => {
            yes(any_value(values, |value| contains(&facts.subject, value)))
        }
        Condition::MyNameInTo => yes(facts.to.iter().any(|value| is_me(value, ctx))),
        Condition::MyNameInCc => yes(facts.cc.iter().any(|value| is_me(value, ctx))),
        Condition::MyNameInToOrCc => yes(
            facts.to.iter().any(|value| is_me(value, ctx))
                || facts.cc.iter().any(|value| is_me(value, ctx)),
        ),
        Condition::MyNameNotInTo => yes(!facts.to.iter().any(|value| is_me(value, ctx))),
        Condition::OnlyRecipient => {
            let recipients = facts.to.len() + facts.cc.len();
            let only_me = facts
                .to
                .iter()
                .chain(facts.cc.iter())
                .all(|value| is_me(value, ctx));
            yes(recipients > 0 && only_me)
        }
        Condition::ExternalSender => match domain_of(&facts.from_addr) {
            Some(domain) => yes(
                !ctx.my_domains
                    .iter()
                    .any(|mine| mine.eq_ignore_ascii_case(domain)),
            ),
            None => Verdict::No,
        },
        Condition::HasFlag => yes(facts.is_starred),
        Condition::HasAttachment => yes(facts.has_attachments),
        Condition::HasLabel { label_id } => yes(facts.labels.contains(label_id)),
        Condition::HasAnyLabel => yes(!facts.labels.is_empty()),
        Condition::Importance { value } => yes(facts.importance == *value),
        Condition::Sensitivity { value } => yes(facts.sensitivity == *value),
        Condition::SubjectOrBodyContains { values } => {
            if any_value(values, |value| contains(&facts.subject, value)) {
                return Verdict::Yes;
            }
            match &facts.body {
                Some(body) => yes(any_value(values, |value| contains(body, value))),
                None => Verdict::NeedMessage,
            }
        }
        Condition::BodyContains { values } => match &facts.body {
            Some(body) => yes(any_value(values, |value| contains(body, value))),
            None => Verdict::NeedMessage,
        },
        Condition::MentionedInBody => match &facts.body {
            Some(body) => yes(mentions_me(body, ctx)),
            None => Verdict::NeedMessage,
        },
        Condition::OnBehalfOf => match &facts.raw {
            Some(raw) => {
                let sender = header_value(raw, "sender");
                let from = header_value(raw, "from");
                let sender = sender.as_deref().map(extract_address);
                let from = from.as_deref().map(extract_address);
                yes(matches!((sender, from), (Some(sender), Some(from)) if !sender.eq_ignore_ascii_case(&from)))
            }
            None => Verdict::NeedMessage,
        },
        Condition::HeaderContains { name, values } => match &facts.raw {
            Some(raw) => {
                let actual = header_value(raw, name);
                yes(actual
                    .map(|actual| any_value(values, |value| contains(&actual, value)))
                    .unwrap_or(false))
            }
            None => Verdict::NeedMessage,
        },
        Condition::MessageType { value } => match &facts.raw {
            Some(raw) => yes(message_type(raw) == *value),
            None => Verdict::NeedMessage,
        },
    }
}

/// First occurrence of a header, with folded continuation lines joined.
pub fn header_value(raw: &[u8], name: &str) -> Option<String> {
    let text = String::from_utf8_lossy(raw);
    let header_block = text
        .split("\r\n\r\n")
        .next()
        .unwrap_or(&text)
        .split("\n\n")
        .next()
        .unwrap_or(&text);
    let mut current: Option<String> = None;
    let mut value = String::new();
    for line in header_block.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.starts_with(' ') || line.starts_with('\t') {
            if current.is_some() {
                value.push(' ');
                value.push_str(line.trim());
            }
            continue;
        }
        if let Some(found) = current.take() {
            if found.eq_ignore_ascii_case(name) {
                return Some(value.trim().to_string());
            }
        }
        if let Some((header, rest)) = line.split_once(':') {
            value = rest.trim().to_string();
            current = Some(header.trim().to_string());
        } else {
            current = None;
            value.clear();
        }
    }
    if let Some(found) = current {
        if found.eq_ignore_ascii_case(name) {
            return Some(value.trim().to_string());
        }
    }
    None
}

fn extract_address(value: &str) -> String {
    let trimmed = value.trim();
    if let (Some(start), Some(end)) = (trimmed.rfind('<'), trimmed.rfind('>')) {
        if end > start {
            return trimmed[start + 1..end].trim().to_string();
        }
    }
    trimmed.to_string()
}

fn message_type(raw: &[u8]) -> MessageType {
    if contains_bytes(raw, b"text/calendar") {
        MessageType::Invitation
    } else if contains_bytes(raw, b"disposition-notification") {
        MessageType::Receipt
    } else {
        MessageType::Normal
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

/// Fill the body-dependent facts from a fetched message.
pub fn apply_raw(facts: &mut Facts, raw: &[u8]) {
    let parsed = parse::parse_message(raw);
    let body = if !parsed.text_body.trim().is_empty() {
        parsed.text_body
    } else {
        parsed
            .html_body
            .as_deref()
            .map(parse::html_body_to_text)
            .unwrap_or_default()
    };
    if parsed
        .attachments
        .iter()
        .any(|part| !parse::is_inline_body_part(&part.mime_type, part.is_inline))
    {
        facts.has_attachments = true;
    }
    facts.raw = Some(raw.to_vec());
    facts.body = Some(body);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{Action, Condition, Priority, Sensitivity};

    fn ctx() -> Context<'static> {
        Context {
            my_addresses: Box::leak(vec!["me@example.com".to_string()].into_boxed_slice()),
            my_names: Box::leak(vec!["Nicole".to_string()].into_boxed_slice()),
            my_domains: Box::leak(vec!["example.com".to_string()].into_boxed_slice()),
        }
    }

    fn rule(conditions: Vec<Condition>) -> Rule {
        Rule {
            id: 1,
            account_id: 1,
            name: "test".into(),
            enabled: true,
            position: 0,
            stop_processing: false,
            conditions,
            exceptions: Vec::new(),
            actions: vec![Action::MarkRead],
        }
    }

    fn facts() -> Facts {
        Facts {
            from_addr: "Alice <alice@elsewhere.org>".into(),
            from_name: "Alice".into(),
            to: vec!["me@example.com".into()],
            cc: Vec::new(),
            subject: "Invoice 42".into(),
            ..Facts::default()
        }
    }

    #[test]
    fn matches_subject_and_sender() {
        assert_eq!(
            evaluate(
                &rule(vec![
                    Condition::SubjectContains {
                        values: vec!["invoice".into()],
                    },
                    Condition::FromContains {
                        values: vec!["elsewhere".into()],
                    },
                ]),
                &facts(),
                &ctx()
            ),
            Verdict::Yes
        );
    }

    #[test]
    fn and_requires_every_condition() {
        assert_eq!(
            evaluate(
                &rule(vec![
                    Condition::SubjectContains {
                        values: vec!["invoice".into()],
                    },
                    Condition::SubjectContains {
                        values: vec!["receipt".into()],
                    },
                ]),
                &facts(),
                &ctx()
            ),
            Verdict::No
        );
    }

    #[test]
    fn exceptions_veto_a_match() {
        let mut rule = rule(vec![Condition::SubjectContains {
                        values: vec!["invoice".into()],
                    }]);
        rule.exceptions = vec![Condition::FromContains {
                        values: vec!["alice".into()],
                    }];
        assert_eq!(evaluate(&rule, &facts(), &ctx()), Verdict::No);
    }

    #[test]
    fn body_conditions_ask_for_the_message_then_answer() {
        let body_rule = rule(vec![Condition::BodyContains {
            values: vec!["payment".into()],
        }]);
        assert_eq!(evaluate(&body_rule, &facts(), &ctx()), Verdict::NeedMessage);
        let mut fetched = facts();
        fetched.body = Some("thanks for the payment".into());
        assert_eq!(evaluate(&body_rule, &fetched, &ctx()), Verdict::Yes);
    }

    #[test]
    fn several_values_match_any_of_them() {
        let sender = rule(vec![Condition::FromContains {
            values: vec!["nobody@example.net".into(), "alice@elsewhere.org".into()],
        }]);
        assert_eq!(evaluate(&sender, &facts(), &ctx()), Verdict::Yes);
        let subject = rule(vec![Condition::SubjectContains {
            values: vec!["receipt".into(), "statement".into()],
        }]);
        assert_eq!(evaluate(&subject, &facts(), &ctx()), Verdict::No);
    }

    #[test]
    fn subject_short_circuits_subject_or_body() {
        let cond = rule(vec![Condition::SubjectOrBodyContains {
            values: vec!["invoice".into()],
        }]);
        assert_eq!(evaluate(&cond, &facts(), &ctx()), Verdict::Yes);
    }

    #[test]
    fn recipient_conditions() {
        assert_eq!(
            evaluate(&rule(vec![Condition::MyNameInTo]), &facts(), &ctx()),
            Verdict::Yes
        );
        assert_eq!(
            evaluate(&rule(vec![Condition::MyNameInCc]), &facts(), &ctx()),
            Verdict::No
        );
        assert_eq!(
            evaluate(&rule(vec![Condition::OnlyRecipient]), &facts(), &ctx()),
            Verdict::Yes
        );
        let mut many = facts();
        many.cc = vec!["other@example.org".into()];
        assert_eq!(
            evaluate(&rule(vec![Condition::OnlyRecipient]), &many, &ctx()),
            Verdict::No
        );
    }

    #[test]
    fn external_sender_uses_account_domains() {
        assert_eq!(
            evaluate(&rule(vec![Condition::ExternalSender]), &facts(), &ctx()),
            Verdict::Yes
        );
        let mut internal = facts();
        internal.from_addr = "me@example.com".into();
        assert_eq!(
            evaluate(&rule(vec![Condition::ExternalSender]), &internal, &ctx()),
            Verdict::No
        );
    }

    #[test]
    fn metadata_conditions() {
        let mut flagged = facts();
        flagged.is_starred = true;
        flagged.has_attachments = true;
        flagged.importance = Priority::High;
        flagged.sensitivity = Sensitivity::Private;
        flagged.labels = vec![3];
        assert_eq!(
            evaluate(&rule(vec![Condition::HasFlag]), &flagged, &ctx()),
            Verdict::Yes
        );
        assert_eq!(
            evaluate(&rule(vec![Condition::HasAttachment]), &flagged, &ctx()),
            Verdict::Yes
        );
        assert_eq!(
            evaluate(
                &rule(vec![Condition::Importance {
                    value: Priority::High
                }]),
                &flagged,
                &ctx()
            ),
            Verdict::Yes
        );
        assert_eq!(
            evaluate(
                &rule(vec![Condition::Sensitivity {
                    value: Sensitivity::Private
                }]),
                &flagged,
                &ctx()
            ),
            Verdict::Yes
        );
        assert_eq!(
            evaluate(
                &rule(vec![Condition::HasLabel { label_id: 3 }]),
                &flagged,
                &ctx()
            ),
            Verdict::Yes
        );
        assert_eq!(
            evaluate(&rule(vec![Condition::HasAnyLabel]), &flagged, &ctx()),
            Verdict::Yes
        );
    }

    #[test]
    fn header_conditions_read_the_raw_block() {
        let raw = b"From: Alice <alice@elsewhere.org>\r\nSender: Bob <bob@proxy.org>\r\nX-Spam: yes\r\nSubject: hi\r\n\r\nbody";
        let mut fetched = facts();
        fetched.raw = Some(raw.to_vec());
        assert_eq!(header_value(raw, "x-spam").as_deref(), Some("yes"));
        assert_eq!(
            evaluate(&rule(vec![Condition::OnBehalfOf]), &fetched, &ctx()),
            Verdict::Yes
        );
        assert_eq!(
            evaluate(
                &rule(vec![Condition::HeaderContains {
                    name: "X-Spam".into(),
                    values: vec!["yes".into()],
                }]),
                &fetched,
                &ctx()
            ),
            Verdict::Yes
        );
        assert_eq!(
            evaluate(&rule(vec![Condition::OnBehalfOf]), &facts(), &ctx()),
            Verdict::NeedMessage
        );
    }

    #[test]
    fn header_folding_is_joined() {
        let raw = b"Subject: hello\r\n world\r\nX-Test:   spaced\r\n\r\n";
        assert_eq!(header_value(raw, "subject").as_deref(), Some("hello world"));
        assert_eq!(header_value(raw, "x-test").as_deref(), Some("spaced"));
    }

    #[test]
    fn message_type_detects_invitations() {
        let raw = b"Content-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: text/calendar\r\n\r\nBEGIN:VCALENDAR";
        let mut fetched = facts();
        fetched.raw = Some(raw.to_vec());
        assert_eq!(
            evaluate(
                &rule(vec![Condition::MessageType {
                    value: MessageType::Invitation
                }]),
                &fetched,
                &ctx()
            ),
            Verdict::Yes
        );
    }

    #[test]
    fn mentions_my_name_or_address() {
        let mention = rule(vec![Condition::MentionedInBody]);
        let mut fetched = facts();
        fetched.body = Some("please loop in Nicole on this".into());
        assert_eq!(evaluate(&mention, &fetched, &ctx()), Verdict::Yes);
        fetched.body = Some("unrelated".into());
        assert_eq!(evaluate(&mention, &fetched, &ctx()), Verdict::No);
    }

    #[test]
    fn apply_raw_fills_body_and_attachment_flag() {
        let raw = b"From: a@b.c\r\nSubject: hi\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: text/plain\r\n\r\nhello\r\n--x\r\nContent-Type: application/pdf\r\nContent-Disposition: attachment; filename=a.pdf\r\n\r\nPDF\r\n--x--\r\n";
        let mut facts = facts();
        apply_raw(&mut facts, raw);
        assert_eq!(facts.body.as_deref(), Some("hello"));
        assert!(facts.has_attachments);
    }

    #[test]
    fn inline_document_attachments_count_for_attachment_rules() {
        let raw = b"From: a@b.c\r\nSubject: hi\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"Apple-Mail=_1\"\r\n\r\n--Apple-Mail=_1\r\nContent-Type: text/plain\r\n\r\nhello\r\n--Apple-Mail=_1\r\nContent-Disposition: inline;\r\n\tfilename=Preventivo.pdf\r\nContent-Type: application/pdf;\r\n\tname=\"Preventivo.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQK\r\n--Apple-Mail=_1--\r\n";
        let mut facts = facts();
        apply_raw(&mut facts, raw);
        assert!(facts.has_attachments);
    }
}
