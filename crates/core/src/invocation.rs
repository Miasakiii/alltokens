use serde_json::Value;

/// Marker stored in `UsageRecord.notes` for synthetic invocation rows.
pub const NOTE_INVOCATION_TOOL: &str = "invocation:tool";
pub const NOTE_INVOCATION_SKILL: &str = "invocation:skill";

pub fn is_invocation_record(notes: Option<&str>) -> bool {
    notes.is_some_and(|n| n.starts_with("invocation:"))
}

/// Extract agent tool invocation names from a JSON line (Claude transcript, Codex rollout, etc.).
pub fn extract_tool_names_from_json(json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    extract_tool_names(&value)
}

/// Extract Skill names from explicit attribution fields or the Claude `Skill` tool.
pub fn extract_skill_names_from_json(json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    extract_skill_names(&value)
}

pub fn extract_tool_names(value: &Value) -> Vec<String> {
    let mut names = Vec::new();
    walk_tools(value, &mut names);
    names
}

pub fn extract_skill_names(value: &Value) -> Vec<String> {
    let mut names = Vec::new();
    walk_skills(value, &mut names);
    names
}

fn walk_tools(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if is_tool_event_object(map) {
                if let Some(name) = tool_name_from_object(map) {
                    if !name.eq_ignore_ascii_case("skill") {
                        out.push(name);
                    }
                }
                return;
            }
            for child in map.values() {
                walk_tools(child, out);
            }
        }
        Value::Array(items) => {
            for child in items {
                walk_tools(child, out);
            }
        }
        _ => {}
    }
}

fn walk_skills(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if is_skill_event_object(map) {
                if let Some(name) = skill_name_from_object(map) {
                    out.push(name);
                }
                return;
            }
            for child in map.values() {
                walk_skills(child, out);
            }
        }
        Value::Array(items) => {
            for child in items {
                walk_skills(child, out);
            }
        }
        _ => {}
    }
}

fn is_tool_event_object(map: &serde_json::Map<String, Value>) -> bool {
    if map.get("type").and_then(Value::as_str) == Some("tool_use") {
        return true;
    }

    if let Some(payload) = map.get("payload").and_then(Value::as_object) {
        if payload
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(is_tool_event_type)
        {
            return true;
        }
    }

    map.get("type")
        .and_then(Value::as_str)
        .is_some_and(is_tool_event_type)
}

fn is_skill_event_object(map: &serde_json::Map<String, Value>) -> bool {
    for key in ["skill_name", "skillName", "skill_id", "skillId"] {
        if map.get(key).and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty()) {
            return true;
        }
    }

    match map.get("type").and_then(Value::as_str) {
        Some("skill" | "skill_use" | "skill_attribution") => true,
        Some("tool_use") => map
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|n| n.eq_ignore_ascii_case("skill")),
        _ => false,
    }
}

fn tool_name_from_object(map: &serde_json::Map<String, Value>) -> Option<String> {
    let obj_type = map
        .get("type")
        .or_else(|| map.get("payload_type"))
        .and_then(Value::as_str);

    let payload = map.get("payload").and_then(Value::as_object);

    if let Some(payload_type) = payload
        .and_then(|p| p.get("type"))
        .and_then(Value::as_str)
    {
        if is_tool_event_type(payload_type) {
            return name_from_map(payload.unwrap());
        }
    }

    match obj_type {
        Some("tool_use") => name_from_map(map),
        Some(
            t @ ("function_call"
            | "tool_call"
            | "mcp_tool_call"
            | "custom_tool_call"
            | "exec_command"
            | "shell_command"),
        ) => name_from_map(map).or_else(|| Some(normalize_tool_label(t))),
        Some("event_msg") => payload.and_then(name_from_map),
        _ => {
            if obj_type.is_some_and(is_tool_event_type) {
                name_from_map(map)
            } else {
                None
            }
        }
    }
}

fn skill_name_from_object(map: &serde_json::Map<String, Value>) -> Option<String> {
    for key in ["skill_name", "skillName", "skill_id", "skillId"] {
        if let Some(name) = map.get(key).and_then(Value::as_str) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    let obj_type = map.get("type").and_then(Value::as_str);
    if matches!(obj_type, Some("skill" | "skill_use" | "skill_attribution")) {
        return name_from_map(map);
    }

    if map.get("type").and_then(Value::as_str) == Some("tool_use") {
        let tool_name = map.get("name").and_then(Value::as_str)?;
        if tool_name.eq_ignore_ascii_case("skill") {
            return skill_name_from_input(map.get("input"));
        }
    }

    None
}

fn skill_name_from_input(input: Option<&Value>) -> Option<String> {
    let input = input?.as_object()?;
    for key in ["skill", "skill_name", "skillName", "name", "path"] {
        if let Some(name) = input.get(key).and_then(Value::as_str) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn is_tool_event_type(value: &str) -> bool {
    matches!(
        value,
        "tool_use"
            | "function_call"
            | "tool_call"
            | "mcp_tool_call"
            | "custom_tool_call"
            | "exec_command"
            | "shell_command"
            | "command_execution"
    )
}

fn name_from_map(map: &serde_json::Map<String, Value>) -> Option<String> {
    for key in ["name", "tool_name", "tool", "function_name", "command"] {
        if let Some(name) = map.get(key).and_then(Value::as_str) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn normalize_tool_label(fallback: &str) -> String {
    fallback.replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_tool_use_content_block() {
        let json = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{}}]}}"#;
        let names = extract_tool_names_from_json(json);
        assert_eq!(names, vec!["Bash".to_string()]);
    }

    #[test]
    fn claude_multiple_tool_uses_counted() {
        let json = r#"{"message":{"content":[
            {"type":"tool_use","name":"Read","input":{}},
            {"type":"tool_use","name":"Grep","input":{}}
        ]}}"#;
        let names = extract_tool_names_from_json(json);
        assert_eq!(names, vec!["Read".to_string(), "Grep".to_string()]);
    }

    #[test]
    fn codex_function_call_event_msg() {
        let json = r#"{"type":"event_msg","payload":{"type":"function_call","name":"shell","input":{}}}"#;
        let names = extract_tool_names_from_json(json);
        assert_eq!(names, vec!["shell".to_string()]);
    }

    #[test]
    fn codex_mcp_tool_call_payload() {
        let json = r#"{"type":"event_msg","payload":{"type":"mcp_tool_call","tool_name":"search","input":{}}}"#;
        let names = extract_tool_names_from_json(json);
        assert_eq!(names, vec!["search".to_string()]);
    }

    #[test]
    fn skill_attribution_field() {
        let json = r#"{"skill_name":"committing-changes-with-git","type":"assistant"}"#;
        let names = extract_skill_names_from_json(json);
        assert_eq!(names, vec!["committing-changes-with-git".to_string()]);
    }

    #[test]
    fn claude_skill_tool_use_input() {
        let json = r#"{"type":"tool_use","name":"Skill","input":{"skill":"canvas","args":{}}}"#;
        let tools = extract_tool_names_from_json(json);
        let skills = extract_skill_names_from_json(json);
        assert!(tools.is_empty());
        assert_eq!(skills, vec!["canvas".to_string()]);
    }

    #[test]
    fn usage_only_line_has_no_tools() {
        let json = r#"{"timestamp":"2026-07-10T12:00:00Z","model":"claude-sonnet","inputTokens":100}"#;
        assert!(extract_tool_names_from_json(json).is_empty());
        assert!(extract_skill_names_from_json(json).is_empty());
    }

    #[test]
    fn invocation_note_marker() {
        assert!(is_invocation_record(Some(NOTE_INVOCATION_TOOL)));
        assert!(!is_invocation_record(Some("source_quality:detailed")));
    }
}
