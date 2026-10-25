use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};

use crate::ai::AiConfig;
use crate::error::{AppError, AppResult};

const TIMEOUT: Duration = Duration::from_secs(60);

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
        if !api_key.trim().is_empty() {
            let bearer = HeaderValue::from_str(&format!("Bearer {api_key}"))
                .map_err(|_| AppError::Message("AI API key is invalid".into()))?;
            headers.insert(AUTHORIZATION, bearer);
        }

        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .map_err(|_| AppError::Message("AI client is unavailable".into()))?;

        Ok(Self {
            http,
            base_url: base_url.to_string(),
            endpoint: format!("{base_url}/chat/completions"),
            model: config.model.trim().to_string(),
            headers,
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
}
