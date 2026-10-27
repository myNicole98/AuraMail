use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};

use crate::ai::AiConfig;
use crate::error::{AppError, AppResult};

const TIMEOUT: Duration = Duration::from_secs(60);
const ANTHROPIC_HOST: &str = "api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_ANTHROPIC_MAX_TOKENS: u32 = 2048;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    role: String,
    content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }

    pub fn role(&self) -> &str {
        &self.role
    }

    pub fn content(&self) -> &str {
        &self.content
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApiFlavor {
    OpenAi,
    Anthropic,
}

#[derive(Serialize)]
struct AnthropicMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage<'a>>,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    #[serde(default)]
    content: Vec<AnthropicContent>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Option<ResponseMessage>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

#[derive(Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingItem>,
}

#[derive(Deserialize)]
struct EmbeddingItem {
    embedding: Vec<f32>,
    #[serde(default)]
    index: usize,
}

pub struct AiClient {
    http: reqwest::Client,
    base_url: String,
    endpoint: String,
    model: String,
    headers: HeaderMap,
    flavor: ApiFlavor,
}

pub(crate) fn is_anthropic_base_url(base_url: &str) -> bool {
    reqwest::Url::parse(base_url.trim())
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .map(|host| host.eq_ignore_ascii_case(ANTHROPIC_HOST))
        .unwrap_or(false)
}

pub(crate) fn provider_headers(base_url: &str, api_key: &str) -> AppResult<HeaderMap> {
    let mut headers = HeaderMap::new();
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Ok(headers);
    }

    if is_anthropic_base_url(base_url) {
        let key = HeaderValue::from_str(api_key)
            .map_err(|_| AppError::Message("AI API key is invalid".into()))?;
        headers.insert(HeaderName::from_static("x-api-key"), key);
        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );
    } else {
        let bearer = HeaderValue::from_str(&format!("Bearer {api_key}"))
            .map_err(|_| AppError::Message("AI API key is invalid".into()))?;
        headers.insert(AUTHORIZATION, bearer);
    }

    Ok(headers)
}

fn anthropic_request<'a>(
    model: &'a str,
    messages: &'a [ChatMessage],
    max_tokens: u32,
) -> AnthropicRequest<'a> {
    let mut system_parts = Vec::new();
    let mut mapped = Vec::new();
    for message in messages {
        if message.role() == "system" {
            let content = message.content().trim();
            if !content.is_empty() {
                system_parts.push(content);
            }
        } else {
            mapped.push(AnthropicMessage {
                role: message.role(),
                content: message.content(),
            });
        }
    }

    AnthropicRequest {
        model,
        max_tokens,
        system: (!system_parts.is_empty()).then(|| system_parts.join("\n\n")),
        messages: mapped,
    }
}

fn parse_anthropic_response(body: &str) -> AppResult<String> {
    let parsed = serde_json::from_str::<AnthropicResponse>(body)
        .map_err(|_| AppError::Message("AI response was invalid".into()))?;
    let content = parsed
        .content
        .into_iter()
        .filter(|block| block.kind == "text")
        .filter_map(|block| block.text)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    if content.is_empty() {
        let detail = parsed
            .stop_reason
            .map(|reason| format!(" (stop reason: {reason})"))
            .unwrap_or_default();
        return Err(AppError::Message(format!("AI response was empty{detail}")));
    }

    Ok(content)
}

pub fn validate_extra_headers(headers: &[(String, String)]) -> AppResult<()> {
    for (name, value) in headers {
        HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|_| AppError::Message("AI header name is invalid".into()))?;
        HeaderValue::from_str(value.trim())
            .map_err(|_| AppError::Message("AI header value is invalid".into()))?;
    }
    Ok(())
}

impl AiClient {
    #[allow(dead_code)]
    pub fn new(config: AiConfig, api_key: &str) -> Self {
        Self::try_new(config, api_key).expect("valid AI configuration")
    }

    pub fn try_new(config: AiConfig, api_key: &str) -> AppResult<Self> {
        let base_url = config.base_url.trim().trim_end_matches('/');
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            return Err(AppError::Message(
                "AI base URL must start with http:// or https://".into(),
            ));
        }
        if config.model.trim().is_empty() {
            return Err(AppError::Message("AI model is required".into()));
        }
        validate_extra_headers(&config.extra_headers)?;

        let mut headers = HeaderMap::new();
        for (name, value) in &config.extra_headers {
            let name = HeaderName::from_bytes(name.trim().as_bytes())
                .map_err(|_| AppError::Message("AI header name is invalid".into()))?;
            let value = HeaderValue::from_str(value.trim())
                .map_err(|_| AppError::Message("AI header value is invalid".into()))?;
            headers.insert(name, value);
        }
        for (name, value) in provider_headers(base_url, api_key)?.into_iter() {
            if let Some(name) = name {
                if !headers.contains_key(&name) {
                    headers.insert(name, value);
                }
            }
        }

        let flavor = if is_anthropic_base_url(base_url) {
            ApiFlavor::Anthropic
        } else {
            ApiFlavor::OpenAi
        };
        let endpoint = match flavor {
            ApiFlavor::OpenAi => format!("{base_url}/chat/completions"),
            ApiFlavor::Anthropic => format!("{base_url}/messages"),
        };

        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .map_err(|_| AppError::Message("AI client is unavailable".into()))?;

        Ok(Self {
            http,
            base_url: base_url.to_string(),
            endpoint,
            model: config.model.trim().to_string(),
            headers,
            flavor,
        })
    }

    #[allow(dead_code)]
    pub async fn chat(&self, messages: &[ChatMessage]) -> AppResult<String> {
        self.complete(messages, None).await
    }

    pub async fn chat_limited(
        &self,
        messages: &[ChatMessage],
        max_tokens: u32,
    ) -> AppResult<String> {
        self.complete(messages, Some(max_tokens)).await
    }

    pub async fn embed(&self, model: &str, inputs: &[String]) -> AppResult<Vec<Vec<f32>>> {
        if self.flavor == ApiFlavor::Anthropic {
            return Err(AppError::Message(
                "Anthropic does not provide an embeddings API".into(),
            ));
        }
        let model = model.trim();
        if model.is_empty() {
            return Err(AppError::Message("Embedding model is required".into()));
        }
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let body = EmbeddingRequest {
            model,
            input: inputs,
        };
        let response = self
            .http
            .post(format!("{}/embeddings", self.base_url))
            .headers(self.headers.clone())
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    AppError::Message("Embedding request timed out".into())
                } else {
                    AppError::Message("Embedding request failed".into())
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(AppError::Message(format!(
                "Embedding request failed ({})",
                status.as_u16()
            )));
        }

        let mut parsed = response
            .json::<EmbeddingResponse>()
            .await
            .map_err(|_| AppError::Message("Embedding response was invalid".into()))?;
        if parsed.data.len() != inputs.len() {
            return Err(AppError::Message(
                "Embedding response count did not match".into(),
            ));
        }
        parsed.data.sort_by_key(|item| item.index);
        let mut vectors = Vec::with_capacity(parsed.data.len());
        for item in parsed.data {
            if item.embedding.is_empty() {
                return Err(AppError::Message(
                    "Embedding response was empty".into(),
                ));
            }
            vectors.push(item.embedding);
        }
        Ok(vectors)
    }

    async fn complete(
        &self,
        messages: &[ChatMessage],
        max_tokens: Option<u32>,
    ) -> AppResult<String> {
        match self.flavor {
            ApiFlavor::OpenAi => self.complete_openai(messages, max_tokens).await,
            ApiFlavor::Anthropic => self.complete_anthropic(messages, max_tokens).await,
        }
    }

    async fn complete_openai(
        &self,
        messages: &[ChatMessage],
        max_tokens: Option<u32>,
    ) -> AppResult<String> {
        let body = ChatRequest {
            model: &self.model,
            messages,
            max_tokens,
        };
        let response = self
            .http
            .post(&self.endpoint)
            .headers(self.headers.clone())
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    AppError::Message("AI request timed out".into())
                } else {
                    AppError::Message("AI request failed".into())
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(AppError::Message(format!(
                "AI request failed ({})",
                status.as_u16()
            )));
        }

        let parsed = response
            .json::<ChatResponse>()
            .await
            .map_err(|_| AppError::Message("AI response was invalid".into()))?;
        let choice = parsed.choices.into_iter().next();
        let finish_reason = choice
            .as_ref()
            .and_then(|choice| choice.finish_reason.clone());
        let content = choice
            .and_then(|choice| choice.message)
            .and_then(|message| message.content)
            .map(|content| content.trim().to_string())
            .filter(|content| !content.is_empty());
        content.ok_or_else(|| {
            let detail = finish_reason
                .map(|reason| format!(" (finish reason: {reason})"))
                .unwrap_or_default();
            AppError::Message(format!("AI response was empty{detail}"))
        })
    }

    async fn complete_anthropic(
        &self,
        messages: &[ChatMessage],
        max_tokens: Option<u32>,
    ) -> AppResult<String> {
        let body = anthropic_request(
            &self.model,
            messages,
            max_tokens.unwrap_or(DEFAULT_ANTHROPIC_MAX_TOKENS),
        );
        let response = self
            .http
            .post(&self.endpoint)
            .headers(self.headers.clone())
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    AppError::Message("AI request timed out".into())
                } else {
                    AppError::Message("AI request failed".into())
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(AppError::Message(format!(
                "AI request failed ({})",
                status.as_u16()
            )));
        }

        let body = response
            .text()
            .await
            .map_err(|_| AppError::Message("AI response was invalid".into()))?;
        parse_anthropic_response(&body)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{AiClient, ChatMessage};
    use crate::ai::AiConfig;

    fn config(server: &MockServer) -> AiConfig {
        AiConfig {
            base_url: server.uri(),
            model: "test-model".into(),
            embedding_base_url: server.uri(),
            embedding_model: "embed-model".into(),
            extra_headers: vec![("x-client".into(), "auramail".into())],
        }
    }

    fn messages() -> Vec<ChatMessage> {
        vec![ChatMessage::user("Hello")]
    }

    #[test]
    fn omits_authorization_when_no_api_key_is_configured() {
        let client = AiClient::try_new(
            AiConfig {
                base_url: "http://localhost:11434/v1".into(),
                model: "llama3".into(),
                embedding_base_url: String::new(),
                embedding_model: String::new(),
                extra_headers: vec![],
            },
            "",
        )
        .unwrap();
        assert!(!client.headers.contains_key(reqwest::header::AUTHORIZATION));
    }

    #[tokio::test]
    async fn posts_bearer_model_and_messages_then_returns_content() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer secret"))
            .and(header("x-client", "auramail"))
            .and(body_partial_json(json!({
                "model": "test-model",
                "messages": [{"role": "user", "content": "Hello"}]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": "  hi there  "}}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = AiClient::new(config(&server), "secret");
        let reply = client.chat(&messages()).await.unwrap();
        assert_eq!(reply, "hi there");
    }

    #[tokio::test]
    async fn joins_a_trailing_slash_without_duplicating_it() {
        let server = MockServer::start().await;
        let mut settings = config(&server);
        settings.base_url = format!("{}/", server.uri());
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": "ok"}}]
            })))
            .mount(&server)
            .await;

        let client = AiClient::new(settings, "secret");
        assert_eq!(client.chat(&messages()).await.unwrap(), "ok");
    }

    #[tokio::test]
    async fn posts_embedding_inputs_and_returns_vectors_in_index_order() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/embeddings"))
            .and(header("authorization", "Bearer secret"))
            .and(header("x-client", "auramail"))
            .and(body_partial_json(json!({
                "model": "embed-model",
                "input": ["one", "two"]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [
                    {"index": 1, "embedding": [0.3, 0.4]},
                    {"index": 0, "embedding": [0.1, 0.2]}
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = AiClient::new(config(&server), "secret");
        let vectors = client
            .embed("embed-model", &["one".into(), "two".into()])
            .await
            .unwrap();
        assert_eq!(vectors, vec![vec![0.1, 0.2], vec![0.3, 0.4]]);
    }

    #[tokio::test]
    async fn reports_embedding_status_without_leaking_the_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/embeddings"))
            .respond_with(ResponseTemplate::new(404).set_body_string("no embeddings here"))
            .mount(&server)
            .await;

        let client = AiClient::new(config(&server), "secret");
        let error = client
            .embed("embed-model", &["one".into()])
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("404"));
        assert!(!error.contains("no embeddings"));
    }

    #[tokio::test]
    async fn returns_a_safe_error_for_non_success_status() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("secret details"))
            .mount(&server)
            .await;

        let client = AiClient::new(config(&server), "secret");
        let error = client.chat(&messages()).await.unwrap_err().to_string();
        assert!(error.contains("401"));
        assert!(!error.contains("secret details"));
    }

    #[tokio::test]
    async fn errors_when_content_is_missing() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"choices": []})))
            .mount(&server)
            .await;

        let client = AiClient::new(config(&server), "secret");
        assert_eq!(
            client.chat(&messages()).await.unwrap_err().to_string(),
            "AI response was empty"
        );
    }

    #[tokio::test]
    async fn reports_the_finish_reason_when_content_is_missing() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"finish_reason": "length", "message": {"content": ""}}]
            })))
            .mount(&server)
            .await;

        let client = AiClient::new(config(&server), "secret");
        assert_eq!(
            client.chat(&messages()).await.unwrap_err().to_string(),
            "AI response was empty (finish reason: length)"
        );
    }

    #[tokio::test]
    async fn rejects_invalid_extra_header_values() {
        let server = MockServer::start().await;
        let mut settings = config(&server);
        settings.extra_headers = vec![("x-test".into(), "bad\nvalue".into())];
        assert!(AiClient::try_new(settings, "secret").is_err());
    }

    #[test]
    fn detects_anthropic_from_its_host() {
        assert!(super::is_anthropic_base_url("https://api.anthropic.com/v1"));
        assert!(super::is_anthropic_base_url("https://api.anthropic.com/v1/"));
        assert!(!super::is_anthropic_base_url("https://api.openai.com/v1"));
        assert!(!super::is_anthropic_base_url("not a url"));
    }

    #[test]
    fn builds_anthropic_messages_with_system_prompt_and_max_tokens() {
        let messages = vec![
            ChatMessage::system("You are concise."),
            ChatMessage::system("Stay calm."),
            ChatMessage::user("Hello"),
            ChatMessage::assistant("Hi"),
            ChatMessage::user("Weather?"),
        ];

        let body = super::anthropic_request("claude-test", &messages, 256);
        assert_eq!(
            serde_json::to_value(body).unwrap(),
            json!({
                "model": "claude-test",
                "max_tokens": 256,
                "system": "You are concise.\n\nStay calm.",
                "messages": [
                    {"role": "user", "content": "Hello"},
                    {"role": "assistant", "content": "Hi"},
                    {"role": "user", "content": "Weather?"}
                ]
            })
        );
    }

    #[test]
    fn parses_anthropic_text_blocks_and_ignores_other_blocks() {
        let body = r#"{
            "content": [
                {"type": "text", "text": "  hello  "},
                {"type": "tool_use", "id": "tool-1"},
                {"type": "text", "text": "world"}
            ],
            "stop_reason": "end_turn"
        }"#;

        assert_eq!(
            super::parse_anthropic_response(body).unwrap(),
            "hello\nworld"
        );
    }

    #[test]
    fn reports_anthropic_stop_reason_when_no_text_is_returned() {
        let error = super::parse_anthropic_response(
            r#"{"content": [], "stop_reason": "max_tokens"}"#,
        )
        .unwrap_err()
        .to_string();

        assert_eq!(error, "AI response was empty (stop reason: max_tokens)");
    }

    #[test]
    fn uses_anthropic_auth_headers_instead_of_bearer_auth() {
        let headers =
            super::provider_headers("https://api.anthropic.com/v1", "secret").unwrap();
        assert_eq!(headers.get("x-api-key").unwrap(), "secret");
        assert_eq!(headers.get("anthropic-version").unwrap(), "2023-06-01");
        assert!(!headers.contains_key(reqwest::header::AUTHORIZATION));

        let headers =
            super::provider_headers("https://api.openai.com/v1", "secret").unwrap();
        assert_eq!(
            headers.get(reqwest::header::AUTHORIZATION).unwrap(),
            "Bearer secret"
        );
        assert!(!headers.contains_key("x-api-key"));
    }

    #[tokio::test]
    async fn rejects_embeddings_for_anthropic_without_network_access() {
        let client = AiClient::new(
            AiConfig {
                base_url: "https://api.anthropic.com/v1".into(),
                model: "claude-test".into(),
                embedding_base_url: String::new(),
                embedding_model: String::new(),
                extra_headers: vec![],
            },
            "secret",
        );

        let error = client
            .embed("embed-model", &["one".into()])
            .await
            .unwrap_err()
            .to_string();
        assert_eq!(error, "Anthropic does not provide an embeddings API");
    }
}
