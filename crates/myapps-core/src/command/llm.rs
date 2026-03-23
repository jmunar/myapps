use super::{CommandAction, CommandIntent};
use crate::config::Config;
use serde::Serialize;
use std::collections::HashMap;

/// Build the **static** system prompt listing all available actions.
///
/// This part is identical across requests for the same set of deployed apps
/// and contains no per-user or per-request data.  By placing it first in the
/// chatml `<|im_start|>system` block, llama-server's KV cache can skip
/// re-evaluating these tokens on subsequent calls that share the same prefix.
///
/// Dynamic, per-user context (e.g. "Available classrooms: …") is deliberately
/// kept out of here and placed in the user message instead so the cache hit
/// rate is maximised.
pub fn build_system_prompt(actions: &[CommandAction]) -> String {
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
        prompt.push('\n');
    }

    prompt
}

/// Build the **dynamic** user message containing per-user context and input.
pub fn build_user_message(
    actions: &[CommandAction],
    user_input: &str,
    context: &HashMap<String, String>,
) -> String {
    let mut msg = String::new();

    // Append context lines for actions that have dynamic values.
    let mut has_context = false;
    for action in actions {
        let key = format!("{}.{}", action.app, action.name);
        if let Some(ctx) = context.get(&key) {
            msg.push_str(&format!("{key}: {ctx}\n"));
            has_context = true;
        }
    }
    if has_context {
        msg.push('\n');
    }

    msg.push_str(user_input);
    msg
}

/// Assemble the full prompt in chatml format.
///
/// The resulting token sequence is:
/// ```text
/// <|im_start|>system\n{system_prompt}<|im_end|>\n
/// <|im_start|>user\n{user_message}<|im_end|>\n
/// <|im_start|>assistant\n
/// ```
///
/// The system block is static across requests, maximising the cacheable prefix.
pub fn build_chatml_prompt(system: &str, user: &str) -> String {
    format!(
        "<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n"
    )
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

/// Request body for the llama.cpp `/completion` endpoint.
#[derive(Serialize)]
struct CompletionRequest {
    prompt: String,
    temperature: f64,
    n_predict: u32,
    stop: Vec<&'static str>,
    cache_prompt: bool,
    id_slot: i32,
    /// JSON Schema sent as `response_format` so the server constrains output.
    response_format: ResponseFormat,
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

/// Send a completion request to the llama.cpp server and parse the response.
///
/// Uses the raw `/completion` endpoint (not `/v1/chat/completions`) because:
/// - It supports `cache_prompt` + `id_slot` for reliable KV-cache prefix
///   reuse across requests that share the same system prompt.
/// - We apply the chatml template ourselves so the static system block stays
///   byte-identical between calls, maximising the cacheable prefix.
pub async fn run_inference(
    config: &Config,
    prompt: &str,
    actions: &[CommandAction],
) -> Result<CommandIntent, String> {
    let url = format!("{}/completion", config.llama_server_url);

    let request = CompletionRequest {
        prompt: prompt.to_string(),
        temperature: 0.1,
        n_predict: 128,
        stop: vec!["<|im_end|>"],
        cache_prompt: true,
        id_slot: 0,
        response_format: ResponseFormat {
            r#type: "json_schema",
            json_schema: SchemaWrapper {
                schema: build_json_schema(actions),
            },
        },
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

    let content = body["content"]
        .as_str()
        .ok_or("No content in llama server response")?;

    // Strip any <think>...</think> wrapper the model may emit.
    let json_str = if let Some(rest) = content.strip_prefix("<think>") {
        rest.split_once("</think>")
            .map(|(_, after)| after.trim())
            .unwrap_or(content)
    } else {
        content
    };

    serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse LLM JSON: {e} -- raw: {content}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{CommandParam, ParamType};

    #[test]
    fn system_prompt_lists_actions_without_context() {
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
        let system = build_system_prompt(&actions);
        assert!(system.contains("mindflow.capture_thought"));
        assert!(system.contains("content (text)"));
        // Context should NOT appear in the system prompt.
        assert!(!system.contains("Available categories"));
    }

    #[test]
    fn user_message_includes_context_and_input() {
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
        let user = build_user_message(&actions, "capture a thought about groceries", &ctx);
        assert!(user.contains("Available categories: Work, Personal"));
        assert!(user.contains("capture a thought about groceries"));
    }

    #[test]
    fn chatml_prompt_has_correct_structure() {
        let full = build_chatml_prompt("system content", "user content");
        assert!(full.starts_with("<|im_start|>system\nsystem content<|im_end|>"));
        assert!(full.contains("<|im_start|>user\nuser content<|im_end|>"));
        assert!(full.ends_with("<|im_start|>assistant\n"));
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
