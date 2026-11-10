use serde_json::Value;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ChatChunk {
    pub content: Option<String>,
    pub finish_reason: Option<String>,
}

/// Turns any chat response body into a sequence of JSON chunk strings.
///
/// Handles three shapes without looking at `Content-Type`:
/// - standard SSE (`data: {...}\n\n`, ending with `data: [DONE]`),
/// - the hybrid some gateways send (a raw JSON object followed by
///   `data: [DONE]` with no newline between them),
/// - a single plain JSON body from a non-streaming provider.
///
/// Bytes are buffered so a chunk may be split anywhere, including inside a
/// multi-byte UTF-8 character or a JSON string.
#[derive(Debug, Default)]
pub struct SseJsonDecoder {
    buffer: Vec<u8>,
    done: bool,
    saw_data: bool,
}

impl SseJsonDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn saw_data(&self) -> bool {
        self.saw_data
    }

    pub fn finished(&self) -> bool {
        self.done
    }

    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        if self.done {
            return Vec::new();
        }
        self.buffer.extend_from_slice(bytes);
        self.drain()
    }

    pub fn finish(&mut self) -> Vec<String> {
        if self.done {
            self.buffer.clear();
            return Vec::new();
        }
        self.drain()
    }

    fn drain(&mut self) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut cursor = 0usize;
        while cursor < self.buffer.len() {
            let byte = self.buffer[cursor];
            if byte.is_ascii_whitespace() {
                cursor += 1;
                continue;
            }
            if self.buffer[cursor..].starts_with(b"[DONE]") {
                self.done = true;
                cursor += b"[DONE]".len();
                break;
            }
            if self.buffer[cursor..].starts_with(b"data:") {
                self.saw_data = true;
                cursor += b"data:".len();
                continue;
            }
            if byte == b'{' {
                match matching_brace(&self.buffer[cursor..]) {
                    Some(end) => {
                        if let Ok(json) = std::str::from_utf8(&self.buffer[cursor..cursor + end]) {
                            chunks.push(json.to_string());
                        }
                        cursor += end;
                        continue;
                    }
                    None => break,
                }
            }
            // SSE field names, comments, and stray bytes before a payload.
            cursor += 1;
        }
        self.buffer.drain(..cursor);
        if self.done {
            self.buffer.clear();
        }
        chunks
    }
}

/// Returns the byte length of the complete JSON object at the start of
/// `bytes`, or `None` when more bytes are needed.
fn matching_brace(bytes: &[u8]) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, &byte) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn parse_chunk(json: &str) -> Option<ChatChunk> {
    let value: Value = serde_json::from_str(json).ok()?;
    let choice = value.get("choices")?.as_array()?.first()?;
    let content = choice
        .pointer("/delta/content")
        .and_then(Value::as_str)
        .or_else(|| choice.pointer("/message/content").and_then(Value::as_str))
        .filter(|content| !content.is_empty())
        .map(str::to_string);
    let finish_reason = choice
        .get("finish_reason")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(ChatChunk {
        content,
        finish_reason,
    })
}

pub fn is_chat_chunk(json: &str) -> bool {
    serde_json::from_str::<Value>(json)
        .ok()
        .and_then(|value| value.get("choices").map(Value::is_array))
        .unwrap_or(false)
}

pub fn is_error_chunk(json: &str) -> bool {
    serde_json::from_str::<Value>(json)
        .ok()
        .map(|value| value.get("error").is_some())
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Object,
    Array,
}

#[derive(Debug)]
struct Frame {
    kind: Kind,
    path: String,
    expecting_key: bool,
    index: usize,
    pending_key: Option<String>,
}

impl Frame {
    fn new(kind: Kind, path: String) -> Self {
        Self {
            kind,
            path,
            expecting_key: kind == Kind::Object,
            index: 0,
            pending_key: None,
        }
    }
}

#[derive(Debug)]
struct StringState {
    /// `Some` for a value string, `None` while reading an object key.
    path: Option<String>,
    text: String,
}

#[derive(Debug, Default)]
enum Escape {
    #[default]
    None,
    Backslash,
    Unicode(String),
}

/// Streams `(path, text)` deltas out of a partial model JSON document.
///
/// Paths are dot-joined keys and array indices (`answer`, `drafts.0.text`,
/// `options.1`). Only string values are emitted, and text is already decoded,
/// so a caller can render it without knowing JSON. Input may stop anywhere:
/// escapes, unicode escapes, and multi-byte characters can span pushes.
#[derive(Debug, Default)]
pub struct JsonProseScanner {
    started: bool,
    done: bool,
    stack: Vec<Frame>,
    string: Option<StringState>,
    escape: Escape,
    pending_high: Option<u16>,
    in_literal: bool,
}

impl JsonProseScanner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, text: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if self.done {
            return out;
        }
        for character in text.chars() {
            if self.done {
                break;
            }
            if self.string.is_some() {
                self.consume_string_char(character, &mut out);
                continue;
            }
            if !self.started {
                if character == '{' {
                    self.started = true;
                    self.stack.push(Frame::new(Kind::Object, String::new()));
                }
                continue;
            }
            if character.is_whitespace() {
                continue;
            }
            match character {
                '"' => self.start_string(),
                '{' => self.start_container(Kind::Object),
                '[' => self.start_container(Kind::Array),
                ':' => {
                    if let Some(frame) = self.stack.last_mut() {
                        frame.expecting_key = false;
                    }
                    self.in_literal = false;
                }
                ',' => {
                    if let Some(frame) = self.stack.last_mut() {
                        if frame.kind == Kind::Object {
                            frame.expecting_key = true;
                        }
                    }
                    self.in_literal = false;
                }
                '}' | ']' => {
                    self.in_literal = false;
                    self.stack.pop();
                    if self.stack.is_empty() {
                        self.done = true;
                    }
                }
                _ => {
                    if !self.in_literal {
                        self.consume_slot();
                        self.in_literal = true;
                    }
                }
            }
        }
        self.flush_string(&mut out);
        out
    }

    fn start_string(&mut self) {
        let is_key = self
            .stack
            .last()
            .map(|frame| frame.kind == Kind::Object && frame.expecting_key)
            .unwrap_or(false);
        let path = if is_key {
            None
        } else {
            Some(self.consume_slot())
        };
        self.string = Some(StringState {
            path,
            text: String::new(),
        });
    }

    fn start_container(&mut self, kind: Kind) {
        let path = self.consume_slot();
        self.stack.push(Frame::new(kind, path));
        self.in_literal = false;
    }

    fn consume_slot(&mut self) -> String {
        let Some(frame) = self.stack.last_mut() else {
            return String::new();
        };
        match frame.kind {
            Kind::Object => {
                frame.expecting_key = false;
                let key = frame.pending_key.take().unwrap_or_default();
                join_path(&frame.path, &key)
            }
            Kind::Array => {
                let index = frame.index;
                frame.index += 1;
                join_path(&frame.path, &index.to_string())
            }
        }
    }

    fn consume_string_char(&mut self, character: char, out: &mut Vec<(String, String)>) {
        let mut state = self.string.take().expect("open string");
        match std::mem::take(&mut self.escape) {
            Escape::Backslash => {
                if character == 'u' {
                    self.escape = Escape::Unicode(String::new());
                } else {
                    state.text.push(match character {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        'b' => '\u{8}',
                        'f' => '\u{c}',
                        other => other,
                    });
                }
            }
            Escape::Unicode(mut digits) => {
                if character.is_ascii_hexdigit() {
                    digits.push(character);
                    if digits.len() == 4 {
                        let code = u16::from_str_radix(&digits, 16).unwrap_or(0xFFFD);
                        if let Some(decoded) = self.decode_code_unit(code) {
                            state.text.push(decoded);
                        }
                    } else {
                        self.escape = Escape::Unicode(digits);
                    }
                } else {
                    state.text.push('\u{FFFD}');
                }
            }
            Escape::None => {
                if character == '\\' {
                    self.escape = Escape::Backslash;
                } else if character == '"' {
                    self.end_string(state, out);
                    return;
                } else {
                    state.text.push(character);
                }
            }
        }
        self.string = Some(state);
    }

    fn decode_code_unit(&mut self, code: u16) -> Option<char> {
        if (0xD800..=0xDBFF).contains(&code) {
            self.pending_high = Some(code);
            return None;
        }
        if (0xDC00..=0xDFFF).contains(&code) {
            let Some(high) = self.pending_high.take() else {
                return Some('\u{FFFD}');
            };
            let combined = 0x10000 + (((high as u32) - 0xD800) << 10) + ((code as u32) - 0xDC00);
            return Some(char::from_u32(combined).unwrap_or('\u{FFFD}'));
        }
        Some(char::from_u32(code as u32).unwrap_or('\u{FFFD}'))
    }

    fn end_string(&mut self, state: StringState, out: &mut Vec<(String, String)>) {
        match state.path {
            Some(path) => {
                if !state.text.is_empty() {
                    out.push((path, state.text));
                }
            }
            None => {
                if let Some(frame) = self.stack.last_mut() {
                    frame.pending_key = Some(state.text);
                }
            }
        }
    }

    fn flush_string(&mut self, out: &mut Vec<(String, String)>) {
        let Some(state) = self.string.as_mut() else {
            return;
        };
        let Some(path) = state.path.clone() else {
            return;
        };
        if state.text.is_empty() {
            return;
        }
        let text = std::mem::take(&mut state.text);
        out.push((path, text));
    }
}

fn join_path(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}.{child}")
    }
}

#[cfg(test)]
mod tests {
    use super::{is_chat_chunk, is_error_chunk, parse_chunk, JsonProseScanner, SseJsonDecoder};

    fn drain(parts: &[&[u8]]) -> Vec<String> {
        let mut decoder = SseJsonDecoder::new();
        let mut chunks = Vec::new();
        for part in parts {
            chunks.extend(decoder.push(part));
        }
        chunks.extend(decoder.finish());
        chunks
    }

    #[test]
    fn decodes_standard_sse_stream() {
        let body = b"data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
                     data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n\
                     data: [DONE]\n\n";
        assert_eq!(
            drain(&[body]),
            vec![
                r#"{"choices":[{"delta":{"content":"Hel"}}]}"#,
                r#"{"choices":[{"delta":{"content":"lo"}}]}"#,
            ]
        );
    }

    #[test]
    fn decodes_hybrid_body_with_glued_done() {
        let body = br#"{"id":"gw","choices":[{"message":{"content":"Hello"}}]}data: [DONE]"#;
        assert_eq!(
            drain(&[body]),
            vec![r#"{"id":"gw","choices":[{"message":{"content":"Hello"}}]}"#]
        );
    }

    #[test]
    fn decodes_plain_json_body() {
        let body = br#"{"choices":[{"message":{"content":"Hello"}}]}"#;
        assert_eq!(
            drain(&[body]),
            vec![r#"{"choices":[{"message":{"content":"Hello"}}]}"#]
        );
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        let body = b": keep-alive\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n";
        assert_eq!(
            drain(&[body]),
            vec![r#"{"choices":[{"delta":{"content":"Hi"}}]}"#]
        );
    }

    #[test]
    fn handles_split_utf8_across_pushes() {
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"café\"}}]}\n\n".as_bytes();
        let split = body
            .iter()
            .position(|byte| *byte >= 0x80)
            .expect("multi-byte character")
            + 1;
        let chunks = drain(&[&body[..split], &body[split..]]);
        assert_eq!(
            chunks,
            vec!["{\"choices\":[{\"delta\":{\"content\":\"café\"}}]}"]
        );
    }

    #[test]
    fn handles_split_inside_json_string() {
        let body = br#"{"choices":[{"delta":{"content":"a}b"}}]}"#;
        let split = body
            .iter()
            .position(|byte| *byte == b'}')
            .expect("closing brace")
            + 1;
        let chunks = drain(&[&body[..split], &body[split..]]);
        assert_eq!(chunks, vec![r#"{"choices":[{"delta":{"content":"a}b"}}]}"#]);
    }

    #[test]
    fn parse_chunk_prefers_delta_then_message() {
        let delta = parse_chunk(r#"{"choices":[{"delta":{"content":"Hi"}}]}"#).unwrap();
        assert_eq!(delta.content.as_deref(), Some("Hi"));
        assert_eq!(delta.finish_reason, None);

        let message = parse_chunk(r#"{"choices":[{"message":{"content":"Hello"}}]}"#).unwrap();
        assert_eq!(message.content.as_deref(), Some("Hello"));
    }

    #[test]
    fn parse_chunk_reads_finish_reason() {
        let chunk = parse_chunk(r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#).unwrap();
        assert_eq!(chunk.content, None);
        assert_eq!(chunk.finish_reason.as_deref(), Some("stop"));
        assert!(parse_chunk("not json").is_none());
        assert!(parse_chunk(r#"{"choices":[]}"#).is_none());
    }

    #[test]
    fn decoder_reports_data_framing_and_completion() {
        let mut decoder = SseJsonDecoder::new();
        assert!(!decoder.saw_data());
        decoder.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n");
        assert!(decoder.saw_data());
        assert!(!decoder.finished());
        decoder.push(b"data: [DONE]\n\n");
        assert!(decoder.finished());

        let mut plain = SseJsonDecoder::new();
        plain.push(br#"{"choices":[]}"#);
        assert!(!plain.saw_data());
        assert!(!plain.finished());
    }

    #[test]
    fn identifies_chat_and_error_chunks() {
        assert!(is_chat_chunk(r#"{"choices":[]}"#));
        assert!(!is_chat_chunk(r#"{"error":{"message":"bad"}}"#));
        assert!(!is_chat_chunk("not json"));
        assert!(is_error_chunk(r#"{"error":{"message":"bad"}}"#));
        assert!(!is_error_chunk(r#"{"choices":[]}"#));
        assert!(!is_error_chunk("not json"));
    }

    fn delta(path: &str, text: &str) -> (String, String) {
        (path.to_string(), text.to_string())
    }

    #[test]
    fn scanner_streams_top_level_strings_across_pushes() {
        let mut scanner = JsonProseScanner::new();
        assert_eq!(
            scanner.push(r#"{"summary":"Hel"#),
            vec![delta("summary", "Hel")]
        );
        assert_eq!(
            scanner.push("lo\",\"ask\":\"Reply?\"}"),
            vec![delta("summary", "lo"), delta("ask", "Reply?")]
        );
    }

    #[test]
    fn scanner_walks_nested_arrays_and_objects() {
        let mut scanner = JsonProseScanner::new();
        assert_eq!(
            scanner.push(
                r#"{"drafts":[{"tone":"warm","text":"Hi"},{"tone":"direct","text":"Yo"}],"options":["A","B"]}"#
            ),
            vec![
                delta("drafts.0.tone", "warm"),
                delta("drafts.0.text", "Hi"),
                delta("drafts.1.tone", "direct"),
                delta("drafts.1.text", "Yo"),
                delta("options.0", "A"),
                delta("options.1", "B"),
            ]
        );
    }

    #[test]
    fn scanner_decodes_escapes() {
        let mut scanner = JsonProseScanner::new();
        assert_eq!(
            scanner.push(r#"{"answer":"Line\n\"quoted\" \\ ok"}"#),
            vec![delta("answer", "Line\n\"quoted\" \\ ok")]
        );
    }

    #[test]
    fn scanner_handles_split_unicode_escape() {
        let mut scanner = JsonProseScanner::new();
        assert_eq!(scanner.push(r#"{"a":"caf\u00"#), vec![delta("a", "caf")]);
        assert_eq!(scanner.push("e9\"}"), vec![delta("a", "é")]);

        let mut emoji = JsonProseScanner::new();
        assert_eq!(
            emoji.push(r#"{"e":"\ud83d\ude00"}"#),
            vec![delta("e", "😀")]
        );
    }

    #[test]
    fn scanner_skips_prefix_and_trailing_text() {
        let mut scanner = JsonProseScanner::new();
        assert_eq!(
            scanner.push("Here:\n```json\n{\"answer\":\"Hi\"}\n```"),
            vec![delta("answer", "Hi")]
        );
        assert!(scanner.push(" trailing").is_empty());
    }

    #[test]
    fn scanner_streams_truncated_values() {
        let mut scanner = JsonProseScanner::new();
        assert_eq!(
            scanner.push(r#"{"answer":"Par"#),
            vec![delta("answer", "Par")]
        );
        assert_eq!(scanner.push("tial"), vec![delta("answer", "tial")]);
    }

    #[test]
    fn scanner_ignores_non_string_values() {
        let mut scanner = JsonProseScanner::new();
        assert_eq!(
            scanner.push(r#"{"n":1,"b":true,"z":null,"o":{"x":"y"}}"#),
            vec![delta("o.x", "y")]
        );
        assert!(JsonProseScanner::new().push(r#"{"a":""}"#).is_empty());
    }
}
