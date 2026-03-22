use super::{CommandAction, CommandIntent};
use crate::config::Config;
use serde::Serialize;
use std::collections::HashMap;

/// Build the system prompt for the LLM, listing all available actions.
///
/// `context` maps `"app.action"` → a string of available values (e.g.
/// `"Available classrooms: Math 3A, Science 4B"`). These are injected
/// into the prompt so the LLM can pick the right names without a
/// separate listing step.
/// Build the LLM prompt. The action catalog comes first (cacheable across
/// requests) and the user input last so llama-server's KV cache can skip
/// re-evaluating the prefix on repeated calls.
pub fn build_prompt(
    actions: &[CommandAction],
    user_input: &str,
    context: &HashMap<String, String>,
) -> String {
    let mut prompt =
        String::from("Pick the action matching the user input. Reply JSON only.\n\nActions:\n");

    for action in actions {
        let key = format!("{}.{}", action.app, action.name);
        prompt.push_str(&format!("- {key}: {}", action.description));
        if !action.params.is_empty() {
            let params_desc: Vec<String> = action
                .params
                .iter()
                .map(|p| {
                    if p.required {
                        format!("{} ({})", p.name, p.param_type.label())
                    } else {
                        format!("{}? ({})", p.name, p.param_type.label())
                    }
                })
                .collect();
            prompt.push_str(&format!(" [{}]", params_desc.join(", ")));
        }
        if let Some(ctx) = context.get(&key) {
            prompt.push_str(&format!(" — {ctx}"));
        }
        prompt.push('\n');
    }

    prompt.push_str(&format!("\nInput: \"{user_input}\"\nJSON:"));

    prompt
}

/// Build a JSON Schema that constrains the LLM to produce a valid CommandIntent.
fn build_json_schema(actions: &[CommandAction]) -> serde_json::Value {
    let action_names: Vec<serde_json::Value> = actions
        .iter()
        .map(|a| serde_json::Value::String(format!("{}.{}", a.app, a.name)))
        .collect();

    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": action_names,
            },
            "params": {
                "type": "object",
            },
            "confidence": {
                "type": "number",
                "minimum": 0.0,
                "maximum": 1.0,
            },
        },
        "required": ["action", "params", "confidence"],
    })
}

#[derive(Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    messages: Vec<ChatMessage>,
    temperature: f64,
    max_tokens: u32,
    response_format: ResponseFormat,
    /// Disable thinking/reasoning mode so grammar enforcement works.
    enable_thinking: bool,
}

#[derive(Serialize)]
struct ResponseFormat {
    r#type: &'static str,
    json_schema: SchemaWrapper,
}

#[derive(Serialize)]
struct SchemaWrapper {
    schema: serde_json::Value,
}

/// Send a chat completion request to the llama.cpp server and parse the response.
pub async fn run_inference(
    config: &Config,
    prompt: &str,
    actions: &[CommandAction],
) -> Result<CommandIntent, String> {
    let url = format!("{}/v1/chat/completions", config.llama_server_url);

    let request = ChatRequest {
        messages: vec![ChatMessage {
            role: "user",
            content: prompt.to_string(),
        }],
        temperature: 0.1,
        max_tokens: 128,
        response_format: ResponseFormat {
            r#type: "json_schema",
            json_schema: SchemaWrapper {
                schema: build_json_schema(actions),
            },
        },
        enable_thinking: false,
    };

    tracing::debug!("LLM prompt:\n{prompt}");

    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Failed to reach llama server: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("llama server returned {status}: {body}"));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse llama server response: {e}"))?;

    tracing::debug!("LLM full response: {body}");

    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("No content in llama server response")?;

    serde_json::from_str(content)
        .map_err(|e| format!("Failed to parse LLM JSON: {e} — raw: {content}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{CommandParam, ParamType};

    #[test]
    fn prompt_includes_actions_and_context() {
        static PARAMS: &[CommandParam] = &[CommandParam {
            name: "content",
            description: "The thought content",
            param_type: ParamType::Text,
            required: true,
        }];
        let actions = vec![CommandAction {
            app: "mindflow",
            name: "capture_thought",
            description: "Capture a new thought",
            params: PARAMS,
        }];
        let mut ctx = HashMap::new();
        ctx.insert(
            "mindflow.capture_thought".to_string(),
            "Available categories: Work, Personal".to_string(),
        );
        let prompt = build_prompt(&actions, "capture a thought about groceries", &ctx);
        assert!(prompt.contains("mindflow.capture_thought"));
        assert!(prompt.contains("content (text)"));
        assert!(prompt.contains("Available categories: Work, Personal"));
        assert!(prompt.contains("capture a thought about groceries"));
    }

    #[test]
    fn json_schema_includes_action_enum() {
        static PARAMS: &[CommandParam] = &[];
        let actions = vec![CommandAction {
            app: "voice_to_text",
            name: "list_jobs",
            description: "List jobs",
            params: PARAMS,
        }];
        let schema = build_json_schema(&actions);
        let enums = &schema["properties"]["action"]["enum"];
        assert_eq!(enums[0], "voice_to_text.list_jobs");
    }
}
