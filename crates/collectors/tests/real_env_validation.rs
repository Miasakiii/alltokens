//! Real environment validation tests.
//! These tests verify collectors against actual local data on this machine.
//! They skip gracefully when the tool is not installed.

use alltokens_collectors::claude_code::ClaudeCodeCollector;
use alltokens_collectors::cursor::CursorCollector;
use alltokens_collectors::Collector;

#[tokio::test]
async fn claude_code_real_env_collects_records() {
    let collector = ClaudeCodeCollector::new();
    if !collector.is_available() {
        eprintln!("SKIP: Claude Code not detected on this machine");
        return;
    }
    let records = collector.collect(None).await.unwrap();
    assert!(
        !records.is_empty(),
        "Claude Code detected but returned 0 records"
    );

    // Validate record structure
    for r in &records {
        assert_eq!(r.collector, "claude_code");
        assert!(
            !r.provider.is_empty() || r.notes.is_some(),
            "record must have provider or be invocation: model={:?}, notes={:?}",
            r.model,
            r.notes
        );
        // Usage records (non-invocation) must have tokens
        if r.notes.is_none() {
            assert!(
                r.input_tokens > 0 || r.output_tokens > 0 || r.cache_read_tokens > 0,
                "usage record has zero tokens: model={:?}, source={:?}",
                r.model,
                r.source_file
            );
        }
        assert!(r.source_file.is_some());
    }

    let usage_records: Vec<_> = records.iter().filter(|r| r.total_tokens > 0).collect();
    eprintln!(
        "Claude Code: {} usage records, {} total records (incl. invocations)",
        usage_records.len(),
        records.len()
    );

    // Check model diversity
    let mut models: Vec<&str> = usage_records.iter().map(|r| r.model.as_str()).collect();
    models.sort();
    models.dedup();
    eprintln!("Models found: {:?}", &models[..models.len().min(10)]);
    assert!(!models.is_empty(), "should find at least one model");

    // Check provider identification
    let mut providers: Vec<&str> = usage_records
        .iter()
        .map(|r| r.provider.as_str())
        .collect();
    providers.sort();
    providers.dedup();
    eprintln!("Providers found: {:?}", providers);
}

#[tokio::test]
async fn claude_code_real_env_incremental_since() {
    let collector = ClaudeCodeCollector::new();
    if !collector.is_available() {
        return;
    }

    // Collect all records first
    let all = collector.collect(None).await.unwrap();
    if all.is_empty() {
        return;
    }

    // Use the midpoint timestamp as `since` filter
    let mid_idx = all.len() / 2;
    let since = all[mid_idx].timestamp;
    let filtered = collector.collect(Some(since)).await.unwrap();

    // Filtered should be a subset
    assert!(
        filtered.len() < all.len(),
        "since filter didn't reduce records: all={} filtered={}",
        all.len(),
        filtered.len()
    );
    // All filtered should be after since
    for r in &filtered {
        assert!(
            r.timestamp > since,
            "record timestamp {:?} should be after since {:?}",
            r.timestamp,
            since
        );
    }
    eprintln!(
        "Incremental: all={}, since midpoint={}, filtered={}",
        all.len(),
        since,
        filtered.len()
    );
}

#[tokio::test]
async fn claude_code_probe_shows_details() {
    let collector = ClaudeCodeCollector::new();
    if !collector.is_available() {
        return;
    }
    let probe = collector.probe_with_quota(false).unwrap();
    assert!(!probe.data_dirs.is_empty());
    assert!(
        probe.usage_files > 0,
        "probe found dirs but no usage files"
    );
    eprintln!(
        "Claude probe: {} dirs, {} files, snapshots: {:?}",
        probe.data_dirs.len(),
        probe.usage_files,
        probe.snapshot_paths
    );
}

#[tokio::test]
async fn cursor_real_env_detection() {
    let collector = CursorCollector::new();
    if !collector.is_available() {
        eprintln!("SKIP: Cursor not detected on this machine");
        return;
    }

    // Cursor is detected (directory exists) but may not have
    // token usage data in the expected format.
    let records = collector.collect(None).await.unwrap();
    eprintln!(
        "Cursor: detected=true, records={} (Cursor stores code-tracking data, not token usage in accessible files)",
        records.len()
    );

    // If records exist, validate structure
    for r in &records {
        assert_eq!(r.collector, "cursor");
        assert!(!r.model.is_empty(), "cursor record missing model");
        assert!(r.input_tokens > 0 || r.output_tokens > 0);
        assert!(r.source_file.is_some());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cursor_probe_shows_details() {
    let collector = CursorCollector::new();
    if !collector.is_available() {
        return;
    }
    let probe = collector.probe().unwrap();
    assert!(probe.detected);
    eprintln!(
        "Cursor probe: {} files, {} sample records, paths: {:?}",
        probe.data_files, probe.sample_records, probe.data_paths
    );
}
