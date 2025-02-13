use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::env;

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

#[derive(Debug, Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ClaudeRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
}

#[derive(Debug, Deserialize)]
pub struct ClaudeResponse {
    pub content: Vec<Content>,
}

#[derive(Debug, Deserialize)]
pub struct Content {
    pub text: String,
}

pub struct ClaudeClient {
    client: reqwest::Client,
    api_key: String,
}

impl ClaudeClient {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let api_key = env::var("ANTHROPIC_API_KEY")
            .map_err(|_| "ANTHROPIC_API_KEY environment variable not set")?;

        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
        })
    }

    pub async fn generate_readme(
        &self,
        system_prompt: &str,
        file_list: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.send_message(system_prompt, file_list).await
    }

    pub async fn send_message(
        &self,
        system_prompt: &str,
        user_message: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("x-api-key", HeaderValue::from_str(&self.api_key)?);
        headers.insert(
            "anthropic-version",
            HeaderValue::from_static("2023-06-01"),
        );

        let messages = vec![
            Message {
                role: "user".to_string(),
                content: format!("{}\n\n{}", system_prompt, user_message),
            },
        ];

        let request = ClaudeRequest {
            model: "claude-3-5-sonnet-20241022".to_string(),
            messages,
            max_tokens: 4096,
        };

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .headers(headers)
            .json(&request)
            .send()
            .await?
            .error_for_status()?;

        let claude_response: ClaudeResponse = response.json().await?;
        Ok(claude_response.content[0].text.clone())
    }
}
