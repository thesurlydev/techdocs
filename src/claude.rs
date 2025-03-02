use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::env;
use tracing::{debug, error, info, instrument};

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
    #[instrument(err)]
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let api_key = match env::var("ANTHROPIC_API_KEY") {
            Ok(key) => {
                info!("Successfully loaded API key from environment");
                key
            }
            Err(e) => {
                error!(?e, "Failed to load ANTHROPIC_API_KEY environment variable");
                return Err("ANTHROPIC_API_KEY environment variable not set".into());
            }
        };

        info!("Initializing Claude client");
        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
        })
    }

    #[instrument(skip(self, system_prompt, file_list), err)]
    pub async fn generate_readme(
        &self,
        system_prompt: &str,
        file_list: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        info!("Generating README");
        debug!(
            system_prompt_length = system_prompt.len(),
            file_list_length = file_list.len(),
            "Starting README generation"
        );
        
        let result = self.send_message(system_prompt, file_list).await;
        
        if result.is_err() {
            error!("README generation failed");
        } else {
            info!("README generated successfully");
        }
        
        result
    }

    #[instrument(
        skip(self, system_prompt, user_message),
        fields(
            system_prompt_length = system_prompt.len(),
            user_message_length = user_message.len(),
        ),
        err
    )]
    pub async fn send_message(
        &self,
        system_prompt: &str,
        user_message: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        info!("Sending message to Claude API");

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        
        match HeaderValue::from_str(&self.api_key) {
            Ok(value) => headers.insert("anthropic-api-key", value),
            Err(e) => {
                error!(?e, "Failed to create API key header");
                return Err(e.into());
            }
        };
        
        headers.insert(
            "anthropic-version",
            HeaderValue::from_static("2023-06-01"),
        );

        debug!("Headers prepared");

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

        debug!(?request.model, max_tokens = request.max_tokens, "Request prepared");

        info!("Sending request to Anthropic API");
        let response = match self
            .client
            .post(ANTHROPIC_API_URL)
            .headers(headers)
            .json(&request)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                error!(?e, "Failed to send request to Anthropic API");
                return Err(e.into());
            }
        };

        let response = match response.error_for_status() {
            Ok(resp) => resp,
            Err(e) => {
                error!(?e, "Received error response from Anthropic API");
                return Err(e.into());
            }
        };

        debug!(status = ?response.status(), "Response received");

        let claude_response: ClaudeResponse = match response.json().await {
            Ok(json) => json,
            Err(e) => {
                error!(?e, "Failed to parse response from Anthropic API");
                return Err(e.into());
            }
        };

        info!(
            response_length = claude_response.content[0].text.len(),
            "Successfully received and parsed response"
        );
        
        Ok(claude_response.content[0].text.clone())
    }
}