use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscoveredProvider {
    pub id: String,
    pub base_url: String,
    pub models: Vec<String>,
}

fn parse_ollama(body: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    value["models"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .filter_map(|model| model["name"].as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn parse_openai(body: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    value["data"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .filter_map(|model| model["id"].as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

async fn probe(
    client: &reqwest::Client,
    id: &str,
    base_url: &str,
    url: &str,
    parser: fn(&str) -> Vec<String>,
) -> Option<DiscoveredProvider> {
    let body = client
        .get(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .text()
        .await
        .ok()?;
    Some(DiscoveredProvider {
        id: id.into(),
        base_url: base_url.into(),
        models: parser(&body),
    })
}

pub async fn discover() -> Vec<DiscoveredProvider> {
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_millis(700))
        .build()
    else {
        return Vec::new();
    };
    let (ollama, lmstudio) = tokio::join!(
        probe(
            &client,
            "ollama",
            "http://localhost:11434/v1",
            "http://127.0.0.1:11434/api/tags",
            parse_ollama,
        ),
        probe(
            &client,
            "lmstudio",
            "http://localhost:1234/v1",
            "http://127.0.0.1:1234/v1/models",
            parse_openai,
        )
    );
    [ollama, lmstudio].into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_ollama, parse_openai};

    #[test]
    fn parses_ollama_models() {
        let models = parse_ollama(r#"{"models":[{"name":"llama3.2"},{"name":"qwen2.5"}]}"#);
        assert_eq!(models, vec!["llama3.2", "qwen2.5"]);
        assert!(parse_ollama("not json").is_empty());
    }

    #[test]
    fn parses_openai_compatible_models() {
        let models = parse_openai(r#"{"data":[{"id":"local-model"},{"id":"other"}]}"#);
        assert_eq!(models, vec!["local-model", "other"]);
        assert!(parse_openai(r#"{"data":[]}"#).is_empty());
    }
}
