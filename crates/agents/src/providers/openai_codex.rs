use std::pin::Pin;

use {
    async_trait::async_trait,
    base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD},
    futures::StreamExt,
    moltis_oauth::{OAuthFlow, TokenStore, load_oauth_config},
    tokio_stream::Stream,
    tracing::debug,
};

use crate::model::{CompletionResponse, LlmProvider, StreamEvent, Usage};

pub struct OpenAiCodexProvider {
    model: String,
    base_url: String,
    client: reqwest::Client,
    token_store: TokenStore,
}

impl OpenAiCodexProvider {
    pub fn new(model: String) -> Self {
        Self {
            model,
            base_url: "https://chatgpt.com/backend-api".to_string(),
            client: reqwest::Client::new(),
            token_store: TokenStore::new(),
        }
    }

    fn get_valid_token(&self) -> anyhow::Result<String> {
        let tokens = self.token_store.load("openai-codex").ok_or_else(|| {
            anyhow::anyhow!(
                "not logged in to openai-codex — run `moltis auth login --provider openai-codex`"
            )
        })?;

        // Check expiry with 5 min buffer
        if let Some(expires_at) = tokens.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if now + 300 >= expires_at {
                // Token expired or expiring — try refresh
                if let Some(ref refresh_token) = tokens.refresh_token {
                    debug!("refreshing openai-codex token");
                    let rt = tokio::runtime::Handle::current();
                    let oauth_config = load_oauth_config("openai-codex")
                        .ok_or_else(|| anyhow::anyhow!("missing oauth config for openai-codex"))?;
                    let flow = OAuthFlow::new(oauth_config);
                    let refresh = refresh_token.clone();
                    let new_tokens = std::thread::scope(|_| rt.block_on(flow.refresh(&refresh)))?;
                    self.token_store.save("openai-codex", &new_tokens)?;
                    return Ok(new_tokens.access_token);
                }
                return Err(anyhow::anyhow!(
                    "openai-codex token expired and no refresh token available"
                ));
            }
        }

        Ok(tokens.access_token)
    }

    fn extract_account_id(jwt: &str) -> anyhow::Result<String> {
        let parts: Vec<&str> = jwt.split('.').collect();
        if parts.len() < 2 {
            anyhow::bail!("invalid JWT format");
        }
        let payload = URL_SAFE_NO_PAD.decode(parts[1]).or_else(|_| {
            // Try with padding
            let padded = match parts[1].len() % 4 {
                2 => format!("{}==", parts[1]),
                3 => format!("{}=", parts[1]),
                _ => parts[1].to_string(),
            };
            base64::engine::general_purpose::STANDARD.decode(&padded)
        })?;
        let claims: serde_json::Value = serde_json::from_slice(&payload)?;
        let account_id = claims["https://api.openai.com/auth"]["chatgpt_account_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing chatgpt_account_id in JWT claims"))?;
        Ok(account_id.to_string())
    }

    fn convert_messages(messages: &[serde_json::Value]) -> Vec<serde_json::Value> {
        messages
            .iter()
            .map(|msg| {
                let role = msg["role"].as_str().unwrap_or("user");
                let content = msg["content"].as_str().unwrap_or("");
                match role {
                    "assistant" => serde_json::json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": content}]
                    }),
                    _ => serde_json::json!({
                        "role": "user",
                        "content": [{"type": "input_text", "text": content}]
                    }),
                }
            })
            .collect()
    }
}

pub fn has_stored_tokens() -> bool {
    TokenStore::new().load("openai-codex").is_some()
}

#[async_trait]
impl LlmProvider for OpenAiCodexProvider {
    fn name(&self) -> &str {
        "openai-codex"
    }

    fn id(&self) -> &str {
        &self.model
    }

    async fn complete(
        &self,
        messages: &[serde_json::Value],
        _tools: &[serde_json::Value],
    ) -> anyhow::Result<CompletionResponse> {
        let token = self.get_valid_token()?;
        let account_id = Self::extract_account_id(&token)?;
        let input = Self::convert_messages(messages);

        let body = serde_json::json!({
            "model": self.model,
            "store": false,
            "stream": false,
            "input": input,
            "text": {"verbosity": "medium"},
            "include": ["reasoning.encrypted_content"],
            "tool_choice": "auto",
        });

        let resp = self
            .client
            .post(format!("{}/codex/responses", self.base_url))
            .header("Authorization", format!("Bearer {token}"))
            .header("chatgpt-account-id", &account_id)
            .header("OpenAI-Beta", "responses=experimental")
            .header("originator", "pi")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;

        let text = resp["output"].as_array().and_then(|outputs| {
            outputs.iter().find_map(|o| {
                if o["type"] == "message" {
                    o["content"]
                        .as_array()
                        .and_then(|c| c.iter().find_map(|item| item["text"].as_str()))
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
        });

        let usage = Usage {
            input_tokens: resp["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
            output_tokens: resp["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
        };

        Ok(CompletionResponse {
            text,
            tool_calls: vec![],
            usage,
        })
    }

    #[allow(clippy::collapsible_if)]
    fn stream(
        &self,
        messages: Vec<serde_json::Value>,
    ) -> Pin<Box<dyn Stream<Item = StreamEvent> + Send + '_>> {
        Box::pin(async_stream::stream! {
            let token = match self.get_valid_token() {
                Ok(t) => t,
                Err(e) => {
                    yield StreamEvent::Error(e.to_string());
                    return;
                }
            };

            let account_id = match Self::extract_account_id(&token) {
                Ok(id) => id,
                Err(e) => {
                    yield StreamEvent::Error(e.to_string());
                    return;
                }
            };

            let input = Self::convert_messages(&messages);

            let body = serde_json::json!({
                "model": self.model,
                "store": false,
                "stream": true,
                "input": input,
                "text": {"verbosity": "medium"},
                "include": ["reasoning.encrypted_content"],
                "tool_choice": "auto",
            });

            let resp = match self
                .client
                .post(format!("{}/codex/responses", self.base_url))
                .header("Authorization", format!("Bearer {token}"))
                .header("chatgpt-account-id", &account_id)
                .header("OpenAI-Beta", "responses=experimental")
                .header("originator", "pi")
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
            {
                Ok(r) => {
                    if let Err(e) = r.error_for_status_ref() {
                        let status = e.status().map(|s| s.as_u16()).unwrap_or(0);
                        let body_text = r.text().await.unwrap_or_default();
                        yield StreamEvent::Error(format!("HTTP {status}: {body_text}"));
                        return;
                    }
                    r
                }
                Err(e) => {
                    yield StreamEvent::Error(e.to_string());
                    return;
                }
            };

            let mut byte_stream = resp.bytes_stream();
            let mut buf = String::new();
            let mut input_tokens: u32 = 0;
            let mut output_tokens: u32 = 0;

            while let Some(chunk) = byte_stream.next().await {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        yield StreamEvent::Error(e.to_string());
                        return;
                    }
                };
                buf.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(pos) = buf.find('\n') {
                    let line = buf[..pos].trim().to_string();
                    buf = buf[pos + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    let Some(data) = line.strip_prefix("data: ") else {
                        continue;
                    };

                    if data == "[DONE]" {
                        yield StreamEvent::Done(Usage { input_tokens, output_tokens });
                        return;
                    }

                    if let Ok(evt) = serde_json::from_str::<serde_json::Value>(data) {
                        let evt_type = evt["type"].as_str().unwrap_or("");

                        match evt_type {
                            "response.output_text.delta" => {
                                if let Some(delta) = evt["delta"].as_str() {
                                    if !delta.is_empty() {
                                        yield StreamEvent::Delta(delta.to_string());
                                    }
                                }
                            }
                            "response.completed" => {
                                if let Some(u) = evt["response"]["usage"].as_object() {
                                    input_tokens = u.get("input_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0) as u32;
                                    output_tokens = u.get("output_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0) as u32;
                                }
                                yield StreamEvent::Done(Usage { input_tokens, output_tokens });
                                return;
                            }
                            "error" | "response.failed" => {
                                let msg = evt["error"]["message"]
                                    .as_str()
                                    .or_else(|| evt["message"].as_str())
                                    .unwrap_or("unknown error");
                                yield StreamEvent::Error(msg.to_string());
                                return;
                            }
                            _ => {}
                        }
                    }
                }
            }
        })
    }
}
