use crate::contacts::ContactInput;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedCard {
    pub uid: String,
    pub display_name: String,
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    pub organization: String,
    pub title: String,
    pub notes: String,
    pub photo: Option<String>,
}

fn unfold(raw: &str) -> Vec<String> {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<String> = Vec::new();
    for line in normalized.split('\n') {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = lines.last_mut() {
                if last.ends_with('=') {
                    last.pop();
                    last.push_str(line.trim_start());
                } else {
                    last.push_str(&line[1..]);
                }
            }
        } else {
            lines.push(line.to_string());
        }
    }
    lines
}

fn is_hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn decode_quoted_printable(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'=' {
            if index + 2 < bytes.len()
                && is_hex(bytes[index + 1])
                && is_hex(bytes[index + 2])
            {
                let hex = &value[index + 1..index + 3];
                out.push(u8::from_str_radix(hex, 16).unwrap_or(b'?'));
                index += 3;
                continue;
            }
            index += 1;
            while index < bytes.len() && (bytes[index] == b' ' || bytes[index] == b'\t') {
                index += 1;
            }
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            other => out.push(other),
        }
    }
    out
}

pub fn parse(raw: &str) -> ParsedCard {
    let mut card = ParsedCard::default();
    let mut formatted_name = String::new();
    let mut structured_name = String::new();
    for line in unfold(raw) {
        let Some((left, raw_value)) = line.split_once(':') else {
            continue;
        };
        let mut parts = left.split(';');
        let Some(key) = parts.next() else {
            continue;
        };
        let key = key.trim().to_ascii_uppercase();
        let params: Vec<String> = parts.map(|part| part.to_ascii_uppercase()).collect();
        let quoted_printable = params
            .iter()
            .any(|param| param == "ENCODING=QUOTED-PRINTABLE");
        let value = if quoted_printable {
            decode_quoted_printable(raw_value)
        } else {
            unescape(raw_value.trim())
        };
        match key.as_str() {
            "UID" => card.uid = value,
            "FN" => formatted_name = value,
            "N" => structured_name = value,
            "EMAIL" => {
                let email = value.trim().to_string();
                if !email.is_empty() {
                    card.emails.push(email);
                }
            }
            "TEL" => {
                let phone = value.trim().to_string();
                if !phone.is_empty() {
                    card.phones.push(phone);
                }
            }
            "ORG" => card.organization = value,
            "TITLE" => card.title = value,
            "NOTE" => card.notes = value,
            "PHOTO" => {
                if value.starts_with("data:") {
                    card.photo = Some(value);
                } else {
                    let encoded = params.iter().any(|param| {
                        param == "ENCODING=B"
                            || param == "ENCODING=BASE64"
                            || param == "ENCODING=B;"
                    });
                    if encoded {
                        let kind = params
                            .iter()
                            .find_map(|param| param.strip_prefix("TYPE="))
                            .unwrap_or("JPEG")
                            .trim_matches('"')
                            .to_ascii_lowercase();
                        card.photo =
                            Some(format!("data:image/{};base64,{}", kind, value.trim()));
                    }
                }
            }
            _ => {}
        }
    }
    card.display_name = if !formatted_name.trim().is_empty() {
        formatted_name
    } else if !structured_name.trim().is_empty() {
        let parts: Vec<&str> = structured_name.split(';').collect();
        let family = parts.first().copied().unwrap_or("").trim();
        let given = parts.get(1).copied().unwrap_or("").trim();
        let suffix = parts.get(4).copied().unwrap_or("").trim();
        [given, family, suffix]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        card.emails.first().cloned().unwrap_or_default()
    };
    card
}

pub fn build(uid: &str, card: &ContactInput) -> String {
    let name = card.display_name.trim();
    let fallback = card.emails.first().map(String::as_str).unwrap_or("");
    let formatted = if name.is_empty() { fallback } else { name };
    let mut lines = vec![
        "BEGIN:VCARD".to_string(),
        "VERSION:4.0".to_string(),
        format!("UID:{}", escape(uid)),
        format!("FN:{}", escape(formatted)),
    ];
    if !formatted.is_empty() {
        let (given, family) = match formatted.rsplit_once(' ') {
            Some((given, family)) => (given, family),
            None => ("", formatted),
        };
        lines.push(format!("N:{};{};;;", escape(family), escape(given)));
    }
    for email in card.emails.iter().filter(|value| !value.trim().is_empty()) {
        lines.push(format!("EMAIL:{}", escape(email.trim())));
    }
    for phone in card.phones.iter().filter(|value| !value.trim().is_empty()) {
        lines.push(format!("TEL:{}", escape(phone.trim())));
    }
    if !card.organization.trim().is_empty() {
        lines.push(format!("ORG:{}", escape(card.organization.trim())));
    }
    if !card.title.trim().is_empty() {
        lines.push(format!("TITLE:{}", escape(card.title.trim())));
    }
    if !card.notes.trim().is_empty() {
        lines.push(format!("NOTE:{}", escape(card.notes.trim())));
    }
    lines.push("END:VCARD".to_string());
    lines.join("\r\n") + "\r\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_folded_vcard_30_with_quoted_printable_name() {
        let raw = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:abc\r\nFN:Nicole Fre=\r\n ga\r\nN:Frega;Nicole;;;\r\nEMAIL;TYPE=WORK:nicole@example.com\r\nEMAIL;TYPE=HOME:nik@example.com\r\nTEL:+39 123\r\nORG:AuraMail\r\nTITLE:Founder\r\nNOTE:Line one\\nLine two\r\nEND:VCARD\r\n";
        let card = parse(raw);
        assert_eq!(card.display_name, "Nicole Frega");
        assert_eq!(card.emails, vec!["nicole@example.com", "nik@example.com"]);
        assert_eq!(card.phones, vec!["+39 123"]);
        assert_eq!(card.organization, "AuraMail");
        assert_eq!(card.title, "Founder");
        assert_eq!(card.notes, "Line one\nLine two");
        assert_eq!(card.uid, "abc");
    }

    #[test]
    fn falls_back_from_fn_to_n_then_to_the_first_email() {
        let card = parse(
            "BEGIN:VCARD\nVERSION:4.0\nN:Frega;Nicole;;;\nEMAIL:nik@example.com\nEND:VCARD\n",
        );
        assert_eq!(card.display_name, "Nicole Frega");
        let card = parse("BEGIN:VCARD\nVERSION:4.0\nEMAIL:nik@example.com\nEND:VCARD\n");
        assert_eq!(card.display_name, "nik@example.com");
    }

    #[test]
    fn built_vcards_escape_delimiters_and_round_trip() {
        let input = ContactInput {
            display_name: "Frega, Nicole".into(),
            emails: vec!["nik@example.com".into()],
            phones: vec![],
            organization: "Aura;Mail".into(),
            title: String::new(),
            notes: "Line one\nLine two".into(),
            address_book_id: None,
        };
        let raw = build("uid-1", &input);
        let card = parse(&raw);
        assert_eq!(card.display_name, "Frega, Nicole");
        assert_eq!(card.organization, "Aura;Mail");
        assert_eq!(card.notes, "Line one\nLine two");
    }

    #[test]
    fn parses_photo_data_and_base64() {
        let card = parse("BEGIN:VCARD\nPHOTO;ENCODING=b;TYPE=PNG:aGk=\nEND:VCARD\n");
        assert_eq!(card.photo.as_deref(), Some("data:image/png;base64,aGk="));
        let card = parse("BEGIN:VCARD\nPHOTO:data:image/jpeg;base64,abc\nEND:VCARD\n");
        assert_eq!(card.photo.as_deref(), Some("data:image/jpeg;base64,abc"));
    }
}
