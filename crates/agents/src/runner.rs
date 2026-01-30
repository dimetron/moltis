use std::sync::Arc;

use {
    anyhow::{Result, bail},
    tracing::{debug, info, warn},
};

use crate::{
    model::{CompletionResponse, LlmProvider},
    tool_registry::ToolRegistry,
};

/// Maximum number of tool-call loop iterations before giving up.
const MAX_ITERATIONS: usize = 25;

/// Result of running the agent loop.
#[derive(Debug)]
pub struct AgentRunResult {
    pub text: String,
    pub iterations: usize,
    pub tool_calls_made: usize,
}

/// Callback for streaming events out of the runner.
pub type OnEvent = Box<dyn Fn(RunnerEvent) + Send + Sync>;

/// Events emitted during the agent run.
#[derive(Debug, Clone)]
pub enum RunnerEvent {
    /// LLM is processing (show a "thinking" indicator).
    Thinking,
    /// LLM finished thinking (hide the indicator).
    ThinkingDone,
    ToolCallStart {
        id: String,
        name: String,
    },
    ToolCallEnd {
        id: String,
        name: String,
        success: bool,
    },
    TextDelta(String),
    Iteration(usize),
}

/// Run the agent loop: send messages to the LLM, execute tool calls, repeat.
pub async fn run_agent_loop(
    provider: Arc<dyn LlmProvider>,
    tools: &ToolRegistry,
    system_prompt: &str,
    user_message: &str,
    on_event: Option<&OnEvent>,
) -> Result<AgentRunResult> {
    let tool_schemas = tools.list_schemas();

    let mut messages: Vec<serde_json::Value> = vec![
        serde_json::json!({
            "role": "system",
            "content": system_prompt,
        }),
        serde_json::json!({
            "role": "user",
            "content": user_message,
        }),
    ];

    let mut iterations = 0;
    let mut total_tool_calls = 0;

    loop {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            warn!("agent loop exceeded max iterations ({})", MAX_ITERATIONS);
            bail!("agent loop exceeded max iterations");
        }

        if let Some(cb) = on_event {
            cb(RunnerEvent::Iteration(iterations));
        }

        debug!(iteration = iterations, "calling LLM");

        if let Some(cb) = on_event {
            cb(RunnerEvent::Thinking);
        }

        let response: CompletionResponse = provider.complete(&messages, &tool_schemas).await?;

        if let Some(cb) = on_event {
            cb(RunnerEvent::ThinkingDone);
        }

        // If no tool calls, return the text response.
        if response.tool_calls.is_empty() {
            let text = response.text.unwrap_or_default();

            info!(
                iterations,
                tool_calls = total_tool_calls,
                "agent loop complete"
            );
            return Ok(AgentRunResult {
                text,
                iterations,
                tool_calls_made: total_tool_calls,
            });
        }

        // Append assistant message with tool calls.
        let tool_calls_json: Vec<serde_json::Value> = response
            .tool_calls
            .iter()
            .map(|tc| {
                serde_json::json!({
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.name,
                        "arguments": tc.arguments.to_string(),
                    }
                })
            })
            .collect();

        let mut assistant_msg = serde_json::json!({
            "role": "assistant",
            "tool_calls": tool_calls_json,
        });
        if let Some(ref text) = response.text {
            assistant_msg["content"] = serde_json::Value::String(text.clone());
        }
        messages.push(assistant_msg);

        // Execute each tool call.
        for tc in &response.tool_calls {
            total_tool_calls += 1;

            if let Some(cb) = on_event {
                cb(RunnerEvent::ToolCallStart {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                });
            }

            let result = if let Some(tool) = tools.get(&tc.name) {
                match tool.execute(tc.arguments.clone()).await {
                    Ok(val) => {
                        if let Some(cb) = on_event {
                            cb(RunnerEvent::ToolCallEnd {
                                id: tc.id.clone(),
                                name: tc.name.clone(),
                                success: true,
                            });
                        }
                        serde_json::json!({ "result": val })
                    },
                    Err(e) => {
                        warn!(tool = %tc.name, error = %e, "tool execution failed");
                        if let Some(cb) = on_event {
                            cb(RunnerEvent::ToolCallEnd {
                                id: tc.id.clone(),
                                name: tc.name.clone(),
                                success: false,
                            });
                        }
                        serde_json::json!({ "error": e.to_string() })
                    },
                }
            } else {
                warn!(tool = %tc.name, "unknown tool");
                if let Some(cb) = on_event {
                    cb(RunnerEvent::ToolCallEnd {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        success: false,
                    });
                }
                serde_json::json!({ "error": format!("unknown tool: {}", tc.name) })
            };

            messages.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": result.to_string(),
            }));
        }
    }
}

/// Convenience wrapper matching the old stub signature.
pub async fn run_agent(_agent_id: &str, _session_key: &str, _message: &str) -> Result<String> {
    bail!("run_agent requires a configured provider and tool registry; use run_agent_loop instead")
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::model::{CompletionResponse, LlmProvider, StreamEvent, ToolCall, Usage},
        async_trait::async_trait,
        std::pin::Pin,
        tokio_stream::Stream,
    };

    /// A mock provider that returns text on the first call.
    struct MockProvider {
        response_text: String,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        fn name(&self) -> &str {
            "mock"
        }

        fn id(&self) -> &str {
            "mock-model"
        }

        async fn complete(
            &self,
            _messages: &[serde_json::Value],
            _tools: &[serde_json::Value],
        ) -> Result<CompletionResponse> {
            Ok(CompletionResponse {
                text: Some(self.response_text.clone()),
                tool_calls: vec![],
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                },
            })
        }

        fn stream(
            &self,
            _messages: Vec<serde_json::Value>,
        ) -> Pin<Box<dyn Stream<Item = StreamEvent> + Send + '_>> {
            Box::pin(tokio_stream::empty())
        }
    }

    #[tokio::test]
    async fn test_simple_text_response() {
        let provider = Arc::new(MockProvider {
            response_text: "Hello!".into(),
        });
        let tools = ToolRegistry::new();
        let result = run_agent_loop(provider, &tools, "You are a test bot.", "Hi", None)
            .await
            .unwrap();
        assert_eq!(result.text, "Hello!");
        assert_eq!(result.iterations, 1);
        assert_eq!(result.tool_calls_made, 0);
    }

    /// Mock provider that makes one tool call then returns text.
    struct ToolCallingProvider {
        call_count: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl LlmProvider for ToolCallingProvider {
        fn name(&self) -> &str {
            "mock"
        }

        fn id(&self) -> &str {
            "mock-model"
        }

        async fn complete(
            &self,
            _messages: &[serde_json::Value],
            _tools: &[serde_json::Value],
        ) -> Result<CompletionResponse> {
            let count = self
                .call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if count == 0 {
                Ok(CompletionResponse {
                    text: None,
                    tool_calls: vec![ToolCall {
                        id: "call_1".into(),
                        name: "echo_tool".into(),
                        arguments: serde_json::json!({"text": "hi"}),
                    }],
                    usage: Usage {
                        input_tokens: 10,
                        output_tokens: 5,
                    },
                })
            } else {
                Ok(CompletionResponse {
                    text: Some("Done!".into()),
                    tool_calls: vec![],
                    usage: Usage {
                        input_tokens: 20,
                        output_tokens: 10,
                    },
                })
            }
        }

        fn stream(
            &self,
            _messages: Vec<serde_json::Value>,
        ) -> Pin<Box<dyn Stream<Item = StreamEvent> + Send + '_>> {
            Box::pin(tokio_stream::empty())
        }
    }

    /// Simple echo tool for testing.
    struct EchoTool;

    #[async_trait]
    impl crate::tool_registry::AgentTool for EchoTool {
        fn name(&self) -> &str {
            "echo_tool"
        }

        fn description(&self) -> &str {
            "Echoes input"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {"text": {"type": "string"}}})
        }

        async fn execute(&self, params: serde_json::Value) -> Result<serde_json::Value> {
            Ok(params)
        }
    }

    #[tokio::test]
    async fn test_tool_call_loop() {
        let provider = Arc::new(ToolCallingProvider {
            call_count: std::sync::atomic::AtomicUsize::new(0),
        });
        let mut tools = ToolRegistry::new();
        tools.register(Box::new(EchoTool));

        let result = run_agent_loop(
            provider,
            &tools,
            "You are a test bot.",
            "Use the tool",
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.text, "Done!");
        assert_eq!(result.iterations, 2);
        assert_eq!(result.tool_calls_made, 1);
    }
}
