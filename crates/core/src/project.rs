use std::path::{Component, Path};

/// Extract a human-readable project name from collector paths or session metadata.
pub fn extract_project_name(source_file: Option<&str>, session_id: Option<&str>) -> Option<String> {
    if let Some(path) = source_file {
        if let Some(name) = extract_from_path(path) {
            return Some(name);
        }
    }

    if let Some(sid) = session_id {
        if looks_like_path(sid) {
            return path_basename(sid);
        }
    }

    None
}

fn extract_from_path(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");

    for marker in ["/.claude/projects/", "/projects/"] {
        if let Some(rest) = normalized
            .find(marker)
            .map(|idx| &normalized[idx + marker.len()..])
        {
            let segment = rest.split('/').next().filter(|s| !s.is_empty())?;
            return Some(decode_claude_project_segment(segment));
        }
    }

    for marker in ["/.zcode/projects/", "/zcode/projects/"] {
        if let Some(rest) = normalized
            .find(marker)
            .map(|idx| &normalized[idx + marker.len()..])
        {
            if let Some(segment) = rest.split('/').next().filter(|s| !s.is_empty()) {
                return Some(segment.to_string());
            }
        }
    }

    if let Some(rest) = normalized
        .find("/archived_sessions/")
        .map(|idx| &normalized[idx + "/archived_sessions/".len()..])
    {
        if let Some(segment) = rest.split('/').next().filter(|s| !s.is_empty()) {
            return Some(segment.to_string());
        }
    }

    if let Some(rest) = normalized
        .find("/sessions/")
        .map(|idx| &normalized[idx + "/sessions/".len()..])
    {
        let mut parts = rest.split('/');
        let _ = parts.next();
        let _ = parts.next();
        let _ = parts.next();
        if let Some(file) = parts.next().filter(|s| !s.is_empty()) {
            if let Some(stem) = Path::new(file).file_stem().and_then(|s| s.to_str()) {
                let name = stem
                    .strip_prefix("rollout-")
                    .or_else(|| stem.strip_prefix("session-"))
                    .unwrap_or(stem);
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
    }

    None
}

fn decode_claude_project_segment(segment: &str) -> String {
    if segment.starts_with('-') {
        let decoded = segment.replace('-', "/");
        if let Some(name) = path_basename(&decoded) {
            return name;
        }
    }
    segment.to_string()
}

fn looks_like_path(value: &str) -> bool {
    value.contains('/') || value.contains('\\')
}

fn path_basename(value: &str) -> Option<String> {
    let normalized = value.replace('\\', "/");
    Path::new(&normalized)
        .components()
        .rev()
        .find_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_project_path_decodes_cwd_segment() {
        let path = "/home/user/.claude/projects/-home-user-dev-alltokens/usage/2026-07-10.jsonl";
        assert_eq!(
            extract_project_name(Some(path), None),
            Some("alltokens".to_string())
        );
    }

    #[test]
    fn zcode_project_path_uses_folder_name() {
        let path = "C:\\Users\\me\\.zcode\\projects\\my-app\\logs\\usage.jsonl";
        assert_eq!(
            extract_project_name(Some(path), None),
            Some("my-app".to_string())
        );
    }

    #[test]
    fn codex_rollout_path_uses_session_stem() {
        let path = "/home/user/.codex/sessions/2026/07/10/rollout-sess-abc.jsonl";
        assert_eq!(
            extract_project_name(Some(path), None),
            Some("sess-abc".to_string())
        );
    }

    #[test]
    fn session_id_path_fallback_uses_basename() {
        assert_eq!(
            extract_project_name(None, Some("/tmp/workspace/demo-app")),
            Some("demo-app".to_string())
        );
    }

    #[test]
    fn missing_context_returns_none() {
        assert_eq!(extract_project_name(None, None), None);
        assert_eq!(
            extract_project_name(Some("/var/log/app.log"), Some("sess-123")),
            None
        );
    }
}
