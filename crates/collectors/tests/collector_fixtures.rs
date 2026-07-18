//! Integration tests: dry-run collector parsers against fixture directories.
//! Real tool installs are not required; validates field mappings programmatically.

use alltokens_collectors::codex::CodexCollector;
use alltokens_collectors::generic::collect_session_json;
use alltokens_collectors::Collector;
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn codex_fixture_parses_jsonl_deltas_and_legacy_json() {
    let root = fixture_root().join("codex");
    let collector = CodexCollector::with_roots(vec![root]);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let records = rt.block_on(collector.collect(None)).unwrap();

    let detailed: Vec<_> = records
        .iter()
        .filter(|r| r.notes.as_deref() == Some("source_quality:detailed"))
        .collect();
    assert!(detailed.len() >= 3, "expected rollout deltas + legacy json");
    assert!(detailed.iter().any(|r| r.model == "gpt-4o"));
    assert!(detailed.iter().any(|r| r.model == "o3-mini"));
    assert!(detailed.iter().any(|r| r.session_id.as_deref() == Some("legacy-json-sess")));
}

#[test]
fn codex_probe_reports_fixture_sources() {
    let root = fixture_root().join("codex");
    let collector = CodexCollector::with_roots(vec![root]);
    let probe = collector.probe_with_quota(false).unwrap();
    assert_eq!(probe.jsonl_files, 2);
    assert!(probe.detailed_records >= 3);
    assert_eq!(probe.coarse_records, 0);
    assert!(probe.quota.is_none());
}

#[test]
fn codex_quota_normalizer_fixture_swapped_windows() {
    use alltokens_collectors::codex_quota::CodexRateLimitNormalizer;
    let raw = std::fs::read_to_string(
        fixture_root()
            .join("codex")
            .join("rate_limits_swapped.json"),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let snapshot = CodexRateLimitNormalizer::normalize(&value).unwrap();
    assert_eq!(snapshot.five_hour.as_ref().unwrap().used_percent, Some(25));
    assert_eq!(snapshot.seven_day.as_ref().unwrap().used_percent, Some(18));
}

#[test]
fn claude_statusline_fixture_parses_five_and_seven_day() {
    use alltokens_collectors::claude_quota::ClaudeStatusLineNormalizer;
    let raw = std::fs::read_to_string(
        fixture_root()
            .join("claude")
            .join("statusline_snapshot_codexu.json"),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let snapshot = ClaudeStatusLineNormalizer::normalize(&value, None).unwrap();
    assert_eq!(snapshot.five_hour.as_ref().unwrap().remaining_percent, Some(76));
    assert_eq!(snapshot.seven_day.as_ref().unwrap().remaining_percent, Some(59));
}

#[test]
fn claude_probe_reads_fixture_snapshot_via_cache_override() {
    use alltokens_collectors::claude_code::ClaudeCodeCollector;
    let dir = tempfile::tempdir().unwrap();
    let snapshot_path = dir
        .path()
        .join("claude-code")
        .join("statusline-snapshot.json");
    std::fs::create_dir_all(snapshot_path.parent().unwrap()).unwrap();
    std::fs::copy(
        fixture_root()
            .join("claude")
            .join("statusline_snapshot_codexu.json"),
        &snapshot_path,
    )
    .unwrap();

    unsafe {
        std::env::set_var("CODEXU_CACHE_OVERRIDE", dir.path());
    }
    let probe = ClaudeCodeCollector::new().probe().unwrap();
    unsafe {
        std::env::remove_var("CODEXU_CACHE_OVERRIDE");
    }

    assert!(probe.quota.is_some());
    assert_eq!(probe.snapshot_paths.len(), 1);
}

#[test]
fn cursor_probe_reports_fixture_sources() {
    use alltokens_collectors::cursor::CursorCollector;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("cursor-usage.jsonl"),
        r#"{"timestamp":"2026-07-10T12:00:00Z","model":"gpt-4o","prompt_tokens":1000,"completion_tokens":500,"total_tokens":1500}"#,
    )
    .unwrap();
    let collector = CursorCollector::with_dirs(vec![dir.path().to_path_buf()]);
    let probe = collector.probe().unwrap();
    assert!(probe.detected);
    assert_eq!(probe.data_files, 1);
    assert_eq!(probe.sample_records, 1);
    assert!(probe.errors.is_empty());
}

#[test]
fn opencode_probe_reports_fixture_sources() {
    use alltokens_collectors::opencode::OpenCodeCollector;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("session.jsonl"),
        r#"{"timestamp":"2026-07-10T12:00:00Z","model":"gpt-4o","inputTokens":100,"outputTokens":50,"totalTokens":150}"#,
    )
    .unwrap();
    let collector = OpenCodeCollector::with_paths(vec![dir.path().to_path_buf()]);
    let probe = collector.probe().unwrap();
    assert!(probe.detected);
    assert_eq!(probe.data_files, 1);
    assert_eq!(probe.sample_records, 1);
}

#[test]
fn windsurf_probe_reports_fixture_sources() {
    use alltokens_collectors::windsurf::WindsurfCollector;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("token-usage.jsonl"),
        r#"{"timestamp":"2026-07-10T12:00:00Z","model":"claude-sonnet-4-20250514","prompt_tokens":200,"completion_tokens":100,"total_tokens":300}"#,
    )
    .unwrap();
    let collector = WindsurfCollector::with_dir(Some(dir.path().to_path_buf()));
    let probe = collector.probe().unwrap();
    assert!(probe.detected);
    assert_eq!(probe.data_files, 1);
    assert_eq!(probe.sample_records, 1);
}

#[test]
fn kimi_fixture_parses_moonshot_models() {
    let dir = fixture_root().join("kimi");
    let records = collect_session_json(&[dir], "kimi", "Kimi CLI", None).unwrap();
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|r| r.collector == "kimi"));
    assert_eq!(records[0].model, "moonshot-v1-8k");
}

#[test]
fn qwen_fixture_parses_dashscope_usage() {
    let dir = fixture_root().join("qwen");
    let records = collect_session_json(&[dir], "qwen", "Qwen CLI", None).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].model, "qwen-plus");
    assert_eq!(records[0].cache_read_tokens, 150);
}

#[test]
fn trae_fixture_parses_deepseek_usage() {
    let dir = fixture_root().join("trae");
    let records = collect_session_json(&[dir], "trae", "Trae", None).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].model, "deepseek-chat");
}

#[test]
fn qoder_fixture_parses_nested_message_usage() {
    let dir = fixture_root().join("qoder");
    let records = collect_session_json(&[dir], "qoder", "Qoder", None).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].input_tokens, 100);
    assert_eq!(records[0].output_tokens, 50);
}
