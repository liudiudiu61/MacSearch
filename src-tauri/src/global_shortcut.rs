#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutPolicy {
    pub physical_id: String,
    pub professional_description: String,
    pub accelerator: String,
    pub window_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutAction {
    ShowAndFocus { window_label: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutPolicyParseError {
    pub field: &'static str,
}

impl ShortcutPolicy {
    pub fn from_json(payload: &str) -> Result<Self, ShortcutPolicyParseError> {
        Ok(Self {
            physical_id: extract_json_string(payload, "physical_id")?,
            professional_description: extract_json_string(payload, "professional_description")?,
            accelerator: extract_json_string(payload, "accelerator")?,
            window_label: extract_json_string(payload, "window_label")?,
        })
    }
}

pub fn handle_search_shortcut(
    policy: &ShortcutPolicy,
    accelerator: &str,
) -> Option<ShortcutAction> {
    if accelerator == policy.accelerator {
        return Some(ShortcutAction::ShowAndFocus {
            window_label: policy.window_label.clone(),
        });
    }

    None
}

fn extract_json_string(
    payload: &str,
    field: &'static str,
) -> Result<String, ShortcutPolicyParseError> {
    let marker = format!("\"{}\"", field);
    let field_start = payload
        .find(&marker)
        .ok_or(ShortcutPolicyParseError { field })?;
    let after_field = &payload[field_start + marker.len()..];
    let colon_index = after_field
        .find(':')
        .ok_or(ShortcutPolicyParseError { field })?;
    let after_colon = after_field[colon_index + 1..].trim_start();
    let value_start = after_colon
        .strip_prefix('"')
        .ok_or(ShortcutPolicyParseError { field })?;
    let value_end = value_start
        .find('"')
        .ok_or(ShortcutPolicyParseError { field })?;
    Ok(value_start[..value_end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_search_shortcut_shows_and_focuses_search_window() {
        let policy = ShortcutPolicy::from_json(include_str!("../../config/shortcut_policy.json"))
            .expect("shortcut policy should parse");

        let action = handle_search_shortcut(&policy, "CommandOrControl+Space");

        assert_eq!(
            action,
            Some(ShortcutAction::ShowAndFocus {
                window_label: "main".to_string()
            })
        );
    }
}
