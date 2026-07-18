//! Claude Code active quota from local statusLine snapshot cache.
//!
//! Reads the same snapshot paths as codexU (`{cache}/codexU/claude-code/statusline-snapshot.json`)
//! plus common statusLine helper outputs. Historical token usage remains in transcript collectors.

use alltokens_core::model::{
    ClaudeQuotaSnapshot, CodexQuotaWindow, CodexQuotaWindowKind,
};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const WINDOW_5H_MINS: i64 = 300;
pub const WINDOW_7D_MINS: i64 = 7 * 24 * 60;
pub const STALE_THRESHOLD_SECS: i64 = 900;

const SOURCE: &str = "claude code statusline snapshot";

/// Parses codexU snapshot wrappers and raw Claude statusLine stdin JSON.
pub struct ClaudeStatusLineNormalizer;

impl ClaudeStatusLineNormalizer {
    pub fn normalize(value: &Value, snapshot_path: Option<&Path>) -> Option<ClaudeQuotaSnapshot> {
        let rate_limits = find_rate_limits(value)?;
        let five_hour = parse_window(rate_limits, &["fiveHour", "five_hour"], WINDOW_5H_MINS);
        let seven_day = parse_window(rate_limits, &["sevenDay", "seven_day"], WINDOW_7D_MINS);

        if five_hour.is_none() && seven_day.is_none() {
            return None;
        }

        let captured_at = read_datetime(
            value
                .get("capturedAt")
                .or(value.get("captured_at"))
                .or(value.get("timestamp")),
        );
        let fetched_at = Utc::now();
        let is_stale = captured_at
            .map(|at| (fetched_at - at).num_seconds() > STALE_THRESHOLD_SECS)
            .unwrap_or(false);

        let source = snapshot_path
            .map(|p| format!("{SOURCE}: {}", p.display()))
            .unwrap_or_else(|| SOURCE.to_string());

        Some(ClaudeQuotaSnapshot {
            fetched_at,
            source,
            snapshot_path: snapshot_path.map(|p| p.to_string_lossy().into_owned()),
            captured_at,
            is_stale,
            five_hour,
            seven_day,
        })
    }
}

fn find_rate_limits(value: &Value) -> Option<&Value> {
    for key in ["rateLimits", "rate_limits"] {
        if let Some(rate_limits) = value.get(key).filter(|v| !v.is_null()) {
            return Some(rate_limits);
        }
    }
    None
}

fn parse_window(
    rate_limits: &Value,
    keys: &[&str],
    window_duration_mins: i64,
) -> Option<CodexQuotaWindow> {
    let window_value = keys
        .iter()
        .find_map(|key| rate_limits.get(*key).filter(|v| !v.is_null()))?;
    let used_percent = read_f64(window_value, &["usedPercentage", "used_percentage"])
        .map(|v| v.round().clamp(0.0, 100.0) as i32);
    let remaining_percent = used_percent.map(|used| (100 - used).clamp(0, 100));
    let resets_at = read_i64(window_value, &["resetsAt", "resets_at"]);

    if used_percent.is_none() && resets_at.is_none() {
        return None;
    }

    let kind = match window_duration_mins {
        WINDOW_5H_MINS => CodexQuotaWindowKind::FiveHour,
        WINDOW_7D_MINS => CodexQuotaWindowKind::SevenDay,
        _ => CodexQuotaWindowKind::Other,
    };

    Some(CodexQuotaWindow {
        kind,
        used_percent,
        remaining_percent,
        window_duration_mins: Some(window_duration_mins),
        resets_at,
    })
}

fn read_f64(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_i64().map(|n| n as f64))
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
}

fn read_i64(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| value.get(*key)).and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_f64().map(|f| f.round() as i64))
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}

fn read_datetime(value: Option<&Value>) -> Option<DateTime<Utc>> {
    let value = value?;
    if let Some(secs) = value.as_i64() {
        return DateTime::from_timestamp(secs, 0);
    }
    if let Some(secs) = value.as_f64() {
        return DateTime::from_timestamp(secs.round() as i64, 0);
    }
    if let Some(text) = value.as_str() {
        if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
            return Some(dt.with_timezone(&Utc));
        }
        if let Ok(dt) = chrono::DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.fZ") {
            return Some(dt.with_timezone(&Utc));
        }
    }
    None
}

/// Candidate statusLine snapshot paths (codexU cache + common helper outputs).
pub fn statusline_snapshot_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();

    let mut push = |path: PathBuf| {
        if seen.insert(path.clone()) {
            paths.push(path);
        }
    };

    if let Ok(override_cache) = std::env::var("CODEXU_CACHE_OVERRIDE") {
        push(
            PathBuf::from(override_cache)
                .join("claude-code")
                .join("statusline-snapshot.json"),
        );
    }

    if let Some(cache) = dirs::cache_dir() {
        push(
            cache
                .join("codexU")
                .join("claude-code")
                .join("statusline-snapshot.json"),
        );
    }

    if let Some(home) = dirs::home_dir() {
        push(
            home.join("Library")
                .join("Caches")
                .join("codexU")
                .join("claude-code")
                .join("statusline-snapshot.json"),
        );
        push(home.join(".claude").join("statusline-snapshot.json"));
        push(home.join(".claude").join("widget-snapshot.json"));
    }

    #[cfg(unix)]
    {
        push(PathBuf::from("/tmp/claude/statusline-raw.json"));
    }

    #[cfg(windows)]
    {
        if let Some(temp) = std::env::var_os("TEMP") {
            push(PathBuf::from(temp).join("claude").join("statusline-raw.json"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            push(
                PathBuf::from(local)
                    .join("codexU")
                    .join("claude-code")
                    .join("statusline-snapshot.json"),
            );
        }
    }

    for base in super::paths::home_dirs() {
        push(
            base.join("AppData")
                .join("Local")
                .join("codexU")
                .join("claude-code")
                .join("statusline-snapshot.json"),
        );
        push(base.join(".claude").join("statusline-snapshot.json"));
    }

    paths
}

/// Read the first available statusLine snapshot on disk.
pub fn read_claude_quota_snapshot() -> Result<ClaudeQuotaSnapshot> {
    let candidates = statusline_snapshot_candidates();
    let mut last_parse_error = None;

    for path in &candidates {
        if !path.exists() {
            continue;
        }
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let value: Value = serde_json::from_str(&raw)
            .with_context(|| format!("invalid JSON in {}", path.display()))?;
        match ClaudeStatusLineNormalizer::normalize(&value, Some(path)) {
            Some(snapshot) => return Ok(snapshot),
            None => last_parse_error = Some(format!("no rate limits in {}", path.display())),
        }
    }

    if let Some(err) = last_parse_error {
        return Err(anyhow!(err));
    }

    let searched = candidates
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(anyhow!(
        "no Claude Code statusline snapshot found (searched: {searched})"
    ))
}

/// Async wrapper for file I/O.
pub async fn fetch_claude_quota() -> Result<ClaudeQuotaSnapshot> {
    tokio::task::spawn_blocking(read_claude_quota_snapshot)
        .await
        .context("claude quota read task failed")?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture(name: &str) -> Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/claude")
            .join(name);
        let raw = fs::read_to_string(path).unwrap();
        serde_json::from_str(&raw).unwrap()
    }

    #[test]
    fn normalizer_parses_codexu_snapshot_wrapper() {
        let mut value = fixture("statusline_snapshot_codexu.json");
        let now = Utc::now().to_rfc3339();
        value
            .as_object_mut()
            .unwrap()
            .insert("capturedAt".to_string(), Value::String(now));
        let snapshot = ClaudeStatusLineNormalizer::normalize(&value, None).unwrap();

        let five = snapshot.five_hour.unwrap();
        assert_eq!(five.used_percent, Some(24));
        assert_eq!(five.remaining_percent, Some(76));
        assert_eq!(five.window_duration_mins, Some(WINDOW_5H_MINS));

        let seven = snapshot.seven_day.unwrap();
        assert_eq!(seven.used_percent, Some(41));
        assert_eq!(seven.remaining_percent, Some(59));
        assert!(!snapshot.is_stale);
        assert!(snapshot.captured_at.is_some());
    }

    #[test]
    fn normalizer_parses_raw_statusline_payload() {
        let value = fixture("statusline_raw.json");
        let snapshot = ClaudeStatusLineNormalizer::normalize(&value, None).unwrap();

        assert_eq!(
            snapshot.five_hour.unwrap().used_percent,
            Some(24)
        );
        assert_eq!(
            snapshot.seven_day.unwrap().used_percent,
            Some(41)
        );
    }

    #[test]
    fn normalizer_marks_stale_snapshots() {
        let mut value = fixture("statusline_snapshot_codexu.json");
        let obj = value.as_object_mut().unwrap();
        obj.insert(
            "capturedAt".to_string(),
            Value::String("2020-01-01T00:00:00Z".to_string()),
        );
        let snapshot = ClaudeStatusLineNormalizer::normalize(&value, None).unwrap();
        assert!(snapshot.is_stale);
    }

    #[test]
    fn read_snapshot_from_temp_fixture_path() {
        let dir = tempfile::tempdir().unwrap();
        let snapshot_path = dir
            .path()
            .join("claude-code")
            .join("statusline-snapshot.json");
        fs::create_dir_all(snapshot_path.parent().unwrap()).unwrap();
        fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/claude/statusline_snapshot_codexu.json"),
            &snapshot_path,
        )
        .unwrap();

        unsafe {
            std::env::set_var("CODEXU_CACHE_OVERRIDE", dir.path());
        }
        let snapshot = read_claude_quota_snapshot().unwrap();
        unsafe {
            std::env::remove_var("CODEXU_CACHE_OVERRIDE");
        }

        assert_eq!(snapshot.five_hour.unwrap().remaining_percent, Some(76));
        assert_eq!(snapshot.snapshot_path.as_deref(), Some(snapshot_path.to_str().unwrap()));
    }
}
