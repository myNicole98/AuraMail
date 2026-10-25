use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::mail::Message;

use super::{AiAuraAskResponse, AiAuraDraft, AiAuraRail};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAuraRail {
    summary: Option<String>,
    ask: Option<String>,
    deadline: Option<String>,
    tone: Option<String>,
    question: Option<String>,
    #[serde(default)]
    options: Vec<String>,
    #[serde(default)]
    drafts: Vec<AiAuraDraft>,
}

#[derive(Debug, Deserialize)]
struct RawAuraAsk {
    answer: Option<String>,
    #[serde(default)]
    drafts: Vec<AiAuraDraft>,
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalized_tone(raw: &str) -> &'static str {
    let value = raw.trim().to_ascii_lowercase();
    if value.contains("warm") || value.contains("friendly") {
        "warm"
    } else if value.contains("concise") || value.contains("short") || value.contains("brief") {
        "concise"
    } else {
        "direct"
    }
}

fn normalize_drafts(raw: Vec<AiAuraDraft>) -> Vec<AiAuraDraft> {
    let mut drafts: Vec<AiAuraDraft> = raw
        .into_iter()
        .filter_map(|draft| {
            let text = draft.text.trim().to_string();
            if text.is_empty() {
                return None;
            }
            Some(AiAuraDraft {
                tone: normalized_tone(&draft.tone).to_string(),
                text,
            })
        })
        .collect();
    let mut used: Vec<String> = Vec::new();
    for draft in drafts.iter_mut() {
        if used.contains(&draft.tone) {
            for candidate in ["direct", "warm", "concise"] {
                if !used.iter().any(|tone| tone == candidate) {
                    draft.tone = candidate.to_string();
                    break;
                }
            }
        }
        used.push(draft.tone.clone());
    }
    drafts
}

fn json_bounds(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(&trimmed[start..=end])
}

pub fn parse_aura_rail(
    raw: &str,
    message_count: u32,
    require_drafts: bool,
) -> AppResult<AiAuraRail> {
    let payload = json_bounds(raw).ok_or_else(|| {
        AppError::Message("Aura returned an unreadable thread brief".into())
    })?;
    let parsed: RawAuraRail = serde_json::from_str(payload)
        .map_err(|_| AppError::Message("Aura returned an unreadable thread brief".into()))?;

    let summary = clean(parsed.summary);
    let ask = clean(parsed.ask);
    let (summary, ask) = match (summary, ask) {
        (Some(summary), Some(ask)) => (summary, ask),
        (Some(summary), None) => (summary.clone(), summary),
        (None, Some(ask)) => (ask.clone(), ask),
        (None, None) => {
            return Err(AppError::Message(
                "Aura returned an unreadable thread brief".into(),
            ));
        }
    };

    let question = if require_drafts {
        clean(parsed.question)
    } else {
        clean(parsed.question)
            .or_else(|| Some(format!("How would you like to respond to: {ask}?")))
    };
    let mut drafts: Vec<AiAuraDraft> = normalize_drafts(parsed.drafts)
        .into_iter()
        .take(3)
        .collect();
    if require_drafts {
        if drafts.len() < 3 {
            return Err(AppError::Message(
                "Aura returned an unreadable thread brief".into(),
            ));
        }
    } else {
        drafts.clear();
    }

    let mut options = Vec::new();
    for option in parsed.options {
        let option = option.trim().to_string();
        if option.is_empty()
            || options
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&option))
        {
            continue;
        }
        options.push(option);
        if options.len() == 3 {
            break;
        }
    }

    Ok(AiAuraRail {
        summary,
        ask,
        deadline: clean(parsed.deadline),
        tone: clean(parsed.tone),
        question,
        options,
        drafts,
        message_count,
    })
}

pub fn parse_aura_ask(raw: &str) -> AppResult<AiAuraAskResponse> {
    let Some(payload) = json_bounds(raw) else {
        let answer = raw.trim().to_string();
        if answer.is_empty() {
            return Err(AppError::Message(
                "Aura returned an unreadable answer".into(),
            ));
        }
        return Ok(AiAuraAskResponse {
            answer,
            drafts: Vec::new(),
        });
    };
    let parsed: RawAuraAsk = serde_json::from_str(payload)
        .map_err(|_| AppError::Message("Aura returned an unreadable answer".into()))?;
    let answer = clean(parsed.answer)
        .ok_or_else(|| AppError::Message("Aura returned an unreadable answer".into()))?;
    let drafts: Vec<AiAuraDraft> = normalize_drafts(parsed.drafts)
        .into_iter()
        .take(3)
        .collect();
    if !drafts.is_empty() && drafts.len() < 3 {
        return Err(AppError::Message(
            "Aura returned unreadable reply options".into(),
        ));
    }
    Ok(AiAuraAskResponse { answer, drafts })
}

pub fn thread_window(messages: &[Message], current_id: i64) -> Vec<Message> {
    let mut window: Vec<Message> = messages.iter().take(8).cloned().collect();
    if !window.iter().any(|message| message.id == current_id) {
        if let Some(current) = messages.iter().find(|message| message.id == current_id) {
            if window.len() == 8 {
                window.pop();
            }
            window.push(current.clone());
        }
    }
    window.sort_by_key(|message| (message.date, message.id));
    window
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_message(id: i64, date: i64) -> Message {
        Message {
            id,
            account_id: 1,
            folder_id: 1,
            uid: id as u32,
            message_id: Some(format!("<{id}@example.com>")),
            references: None,
            thread_key: "thread".into(),
            from_addr: "ada@example.com".into(),
            from_name: "Ada".into(),
            to_addrs: vec!["me@example.com".into()],
            cc_addrs: Vec::new(),
            subject: format!("Subject {id}"),
            date,
            snippet: format!("Snippet {id}"),
            is_seen: false,
            is_starred: false,
            has_attachments: false,
            body_cached: false,
            tab_id: None,
        }
    }

    #[test]
    fn parses_fenced_initial_rail_with_only_a_question() {
        let raw = r#"```json
        {"summary":"Priya wants a discount confirmation.","ask":"Confirm the 12% discount.","deadline":"Thursday","tone":"Direct","question":"Full package or base shoot?","options":["Full package","Base shoot"],"drafts":[]}
        ```"#;
        let rail = parse_aura_rail(raw, 4, false).unwrap();
        assert_eq!(rail.message_count, 4);
        assert!(rail.drafts.is_empty());
        assert_eq!(
            rail.question.as_deref(),
            Some("Full package or base shoot?")
        );
        assert_eq!(rail.options.len(), 2);
        assert_eq!(rail.deadline.as_deref(), Some("Thursday"));
    }

    #[test]
    fn initial_rail_discards_drafts_until_the_question_is_answered() {
        let raw = r#"{"summary":"s","ask":"a","question":"Which package?","drafts":[{"tone":"direct","text":"x"},{"tone":"warm","text":"y"},{"tone":"concise","text":"z"}]}"#;
        let rail = parse_aura_rail(raw, 1, false).unwrap();
        assert!(rail.drafts.is_empty());
    }

    #[test]
    fn initial_rail_falls_back_to_a_contextual_question() {
        let raw = r#"{"summary":"s","ask":"Confirm the date.","drafts":[]}"#;
        let rail = parse_aura_rail(raw, 1, false).unwrap();
        assert!(rail
            .question
            .as_deref()
            .unwrap_or_default()
            .contains("Confirm the date"));
        assert!(rail.drafts.is_empty());
    }

    #[test]
    fn answered_rail_requires_three_drafts() {
        let raw = r#"{"summary":"s","ask":"a","question":null,"options":[],"drafts":[{"tone":"direct","text":"Yes."},{"tone":"warm","text":"Happy to confirm."},{"tone":"concise","text":"Confirmed."}]}"#;
        let rail = parse_aura_rail(raw, 4, true).unwrap();
        assert_eq!(rail.drafts.len(), 3);
        assert_eq!(rail.drafts[2].tone, "concise");
        assert!(rail.question.is_none());
    }

    #[test]
    fn normalizes_duplicate_tones_and_options() {
        let raw = r#"{"summary":"s","ask":"a","options":["One","One","Two"],"drafts":[{"tone":"professional","text":"x"},{"tone":"friendly","text":"y"},{"tone":"short","text":"z"}]}"#;
        let rail = parse_aura_rail(raw, 1, true).unwrap();
        assert_eq!(rail.drafts[0].tone, "direct");
        assert_eq!(rail.drafts[1].tone, "warm");
        assert_eq!(rail.drafts[2].tone, "concise");
        assert_eq!(rail.options, vec!["One".to_string(), "Two".to_string()]);
    }

    #[test]
    fn thread_window_keeps_the_current_message_even_when_it_is_old() {
        let messages: Vec<_> = (1..=10)
            .map(|id| sample_message(id, id as i64))
            .rev()
            .collect();
        let window = thread_window(&messages, 1);
        assert_eq!(window.len(), 8);
        assert!(window.iter().any(|message| message.id == 1));
        assert!(window.windows(2).all(|pair| pair[0].date <= pair[1].date));
    }

    #[test]
    fn parses_reply_guidance_with_three_drafts() {
        let raw = r#"{"answer":"Added three options above.","drafts":[{"tone":"direct","text":"I can attend; please send the agenda."},{"tone":"warm","text":"Happy to join — could you share the agenda first?"},{"tone":"concise","text":"Yes, if you send the agenda."}]}"#;
        let response = parse_aura_ask(raw).unwrap();
        assert_eq!(response.answer, "Added three options above.");
        assert_eq!(response.drafts.len(), 3);
        assert_eq!(response.drafts[2].tone, "concise");
    }

    #[test]
    fn parses_a_thread_question_without_drafts() {
        let raw = r#"{"answer":"The deadline is Thursday.","drafts":[]}"#;
        let response = parse_aura_ask(raw).unwrap();
        assert_eq!(response.answer, "The deadline is Thursday.");
        assert!(response.drafts.is_empty());
    }

    #[test]
    fn accepts_plain_text_chat_answers() {
        let response = parse_aura_ask("The deadline is Thursday.").unwrap();
        assert_eq!(response.answer, "The deadline is Thursday.");
        assert!(response.drafts.is_empty());
    }

    #[test]
    fn rejects_unusable_rail_json() {
        assert!(parse_aura_rail("I cannot help with that.", 1, false).is_err());
        assert!(parse_aura_rail(
            r#"{"summary":"x","ask":"y","drafts":[{"tone":"direct","text":"a"},{"tone":"warm","text":"b"}]}"#,
            1,
            true
        )
        .is_err());
    }

    #[allow(dead_code)]
    fn _keep_sample_message_used() {
        let _ = sample_message(1, 1);
    }
}
