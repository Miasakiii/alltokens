//! Shared probe types and helpers for `alltokens probe`.

use serde::Serialize;

/// Status row for `alltokens probe` (no collector argument).
#[derive(Debug, Clone, Serialize)]
pub struct CollectorProbeStatus {
    pub id: String,
    pub name: String,
    pub detected: bool,
    pub probe_supported: bool,
}

/// Standard probe result for collectors without live quota APIs.
#[derive(Debug, Clone, Serialize)]
pub struct BasicProbeResult {
    pub collector_id: String,
    pub collector_name: String,
    pub detected: bool,
    pub data_paths: Vec<String>,
    pub data_files: usize,
    pub sample_records: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

/// Collectors that implement a detailed `probe` subcommand.
pub fn probe_supported_ids() -> &'static [&'static str] {
    &["codex", "claude_code", "cursor", "opencode", "windsurf"]
}

/// Normalize CLI aliases to canonical collector IDs.
pub fn normalize_probe_collector_id(input: &str) -> &str {
    match input {
        "claude" => "claude_code",
        other => other,
    }
}

/// List all registered collectors with detected / probe-supported flags.
pub fn list_collector_probe_status() -> Vec<CollectorProbeStatus> {
    let supported: std::collections::HashSet<&str> = probe_supported_ids().iter().copied().collect();

    super::register_collectors()
        .into_iter()
        .map(|c| CollectorProbeStatus {
            id: c.id().to_string(),
            name: c.name().to_string(),
            detected: c.is_available(),
            probe_supported: supported.contains(c.id()),
        })
        .collect()
}

pub(crate) fn build_basic_probe_result(
    collector_id: &str,
    collector_name: &str,
    detected: bool,
    data_paths: Vec<String>,
    data_files: usize,
    sample_records: Result<usize, String>,
) -> BasicProbeResult {
    let (sample_records, errors) = match sample_records {
        Ok(n) => (n, Vec::new()),
        Err(e) => (0, vec![e]),
    };

    BasicProbeResult {
        collector_id: collector_id.to_string(),
        collector_name: collector_name.to_string(),
        detected,
        data_paths,
        data_files,
        sample_records,
        errors,
    }
}

pub(crate) fn collect_sample_count<C>(collector: &C) -> Result<usize, String>
where
    C: super::Collector + ?Sized,
{
    if !collector.is_available() {
        return Ok(0);
    }
    // If we're already inside a tokio runtime (e.g. called from `#[tokio::main]` CLI),
    // use block_in_place + current handle to avoid "cannot start a runtime from within a runtime".
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| {
            handle.block_on(collector.collect(None))
                .map(|records| records.len())
                .map_err(|e| e.to_string())
        })
    } else {
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        rt.block_on(collector.collect(None))
            .map(|records| records.len())
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_claude_alias() {
        assert_eq!(normalize_probe_collector_id("claude"), "claude_code");
        assert_eq!(normalize_probe_collector_id("cursor"), "cursor");
    }

    #[test]
    fn list_includes_registered_collectors() {
        let statuses = list_collector_probe_status();
        assert!(statuses.len() >= 20);
        assert!(statuses.iter().any(|s| s.id == "cursor"));
        assert!(statuses.iter().any(|s| s.id == "codex" && s.probe_supported));
        assert!(statuses.iter().any(|s| s.id == "opencode" && s.probe_supported));
    }

    #[test]
    fn basic_probe_result_serializes_errors_when_present() {
        let probe = build_basic_probe_result(
            "cursor",
            "Cursor",
            true,
            vec!["/tmp/cursor".into()],
            2,
            Err("parse failed".into()),
        );
        let json = serde_json::to_string(&probe).unwrap();
        assert!(json.contains("parse failed"));
        assert_eq!(probe.sample_records, 0);
    }
}
