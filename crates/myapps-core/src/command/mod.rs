pub mod llm;
pub mod routes;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Describes a parameter accepted by a command action.
pub struct CommandParam {
    pub name: &'static str,
    pub description: &'static str,
    pub param_type: ParamType,
    pub required: bool,
}

/// Supported parameter types.
pub enum ParamType {
    Text,
    Number,
}

impl ParamType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
        }
    }
}

/// A command action that an app exposes.
pub struct CommandAction {
    pub app: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub params: &'static [CommandParam],
}

/// The intent parsed from the LLM's JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandIntent {
    pub action: String,
    pub params: HashMap<String, serde_json::Value>,
    pub confidence: f64,
}

/// Result of executing a command.
pub struct CommandResult {
    pub success: bool,
    pub message: String,
    /// If set, the frontend should navigate to this URL instead of showing a message.
    pub redirect: Option<String>,
}

impl CommandResult {
    pub fn message(msg: impl Into<String>) -> Self {
        Self {
            success: true,
            message: msg.into(),
            redirect: None,
        }
    }

    pub fn redirect(url: impl Into<String>) -> Self {
        Self {
            success: true,
            message: String::new(),
            redirect: Some(url.into()),
        }
    }
}

/// Collect all command actions from the given apps.
pub fn collect_commands(apps: &[Box<dyn crate::registry::App>]) -> Vec<CommandAction> {
    apps.iter().flat_map(|app| app.commands()).collect()
}

/// Collect dynamic context for the LLM prompt (e.g. available classrooms).
/// Returns a map of `"app.action"` → context string.
pub async fn collect_command_context(
    pool: &sqlx::SqlitePool,
    user_id: i64,
    apps: &[Box<dyn crate::registry::App>],
) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    for app in apps {
        ctx.extend(app.command_context(pool, user_id).await);
    }
    ctx
}

/// Validate a parsed intent against the action catalog.
/// Returns `Ok(())` or an error message describing the validation failure.
pub fn validate_intent(intent: &CommandIntent, actions: &[CommandAction]) -> Result<(), String> {
    let action = actions
        .iter()
        .find(|a| format!("{}.{}", a.app, a.name) == intent.action)
        .ok_or_else(|| format!("Unknown action: {}", intent.action))?;

    for param in action.params {
        if param.required && !intent.params.contains_key(param.name) {
            return Err(format!("Missing required parameter: {}", param.name));
        }
        if let Some(value) = intent.params.get(param.name) {
            match param.param_type {
                ParamType::Text => {
                    if !value.is_string() {
                        return Err(format!("Parameter '{}' must be text", param.name));
                    }
                }
                ParamType::Number => {
                    if !value.is_number() {
                        return Err(format!("Parameter '{}' must be a number", param.name));
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_PARAMS: &[CommandParam] = &[CommandParam {
        name: "content",
        description: "The thought content",
        param_type: ParamType::Text,
        required: true,
    }];

    fn test_actions() -> Vec<CommandAction> {
        vec![CommandAction {
            app: "mindflow",
            name: "capture_thought",
            description: "Capture a new thought",
            params: TEST_PARAMS,
        }]
    }

    #[test]
    fn validate_valid_intent() {
        let intent = CommandIntent {
            action: "mindflow.capture_thought".to_string(),
            params: HashMap::from([(
                "content".to_string(),
                serde_json::Value::String("groceries".to_string()),
            )]),
            confidence: 0.9,
        };
        assert!(validate_intent(&intent, &test_actions()).is_ok());
    }

    #[test]
    fn validate_unknown_action() {
        let intent = CommandIntent {
            action: "unknown.action".to_string(),
            params: HashMap::new(),
            confidence: 0.5,
        };
        let err = validate_intent(&intent, &test_actions()).unwrap_err();
        assert!(err.contains("Unknown action"));
    }

    #[test]
    fn validate_missing_required_param() {
        let intent = CommandIntent {
            action: "mindflow.capture_thought".to_string(),
            params: HashMap::new(),
            confidence: 0.8,
        };
        let err = validate_intent(&intent, &test_actions()).unwrap_err();
        assert!(err.contains("Missing required parameter"));
    }

    #[test]
    fn validate_wrong_param_type() {
        let intent = CommandIntent {
            action: "mindflow.capture_thought".to_string(),
            params: HashMap::from([(
                "content".to_string(),
                serde_json::Value::Number(serde_json::Number::from(42)),
            )]),
            confidence: 0.8,
        };
        let err = validate_intent(&intent, &test_actions()).unwrap_err();
        assert!(err.contains("must be text"));
    }
}
