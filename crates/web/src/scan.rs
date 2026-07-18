use alltokens_core::pricing::PricingEngine;
use alltokens_core::storage::Storage;
use anyhow::Result;
use chrono::Utc;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub total: usize,
    pub by_collector: Vec<(String, usize)>,
}

pub async fn run_scan(storage: &Storage, pricing: &PricingEngine) -> Result<ScanResult> {
    let collectors = alltokens_collectors::register_collectors();
    let mut total = 0usize;
    let mut by_collector = Vec::new();

    for collector in &collectors {
        if !storage.is_collector_enabled(collector.id())? {
            continue;
        }
        if !collector.is_available() {
            continue;
        }

        let since = storage.get_last_scan(collector.id())?;
        println!("🔍 Scanning {}...", collector.name());
        match collector.collect(since).await {
            Ok(mut records) => {
                for record in &mut records {
                    pricing.calculate_cost(record);
                }
                let count = records.len();
                if !records.is_empty() {
                    storage.insert_records(&records)?;
                }
                let now = Utc::now().to_rfc3339();
                storage.set_collector_state(collector.id(), &now, "{}")?;
                if count > 0 {
                    println!("   ✅ {count} records collected");
                }
                by_collector.push((collector.name().to_string(), count));
                total += count;
            }
            Err(e) => {
                println!("   ❌ Error: {e}");
                by_collector.push((collector.name().to_string(), 0));
            }
        }
    }

    Ok(ScanResult { total, by_collector })
}

/// Outcome of one background maintenance cycle: a scan pass followed by
/// retention-based pruning of stale records.
#[derive(Debug, Serialize)]
pub struct MaintenanceResult {
    pub scan: ScanResult,
    /// Number of records removed by the retention purge (0 if disabled).
    pub purged: usize,
    /// Retention window in effect (days). 0 = keep everything.
    pub retention_days: u32,
}

/// Run one full maintenance cycle: scan all enabled collectors, then purge
/// records older than the configured `retention_days` (0 disables pruning).
pub async fn run_maintenance_cycle(
    storage: &Storage,
    pricing: &PricingEngine,
) -> Result<MaintenanceResult> {
    let scan = run_scan(storage, pricing).await?;
    let retention_days = storage.get_data_config()?.retention_days;
    let purged = storage.purge_records_older_than_days(retention_days)?;
    if purged > 0 {
        println!("\u{1f9f9} Purged {purged} records older than {retention_days} days");
    }
    Ok(MaintenanceResult { scan, purged, retention_days })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alltokens_core::model::{DataConfig, UsageRecord};

    fn old_record(days_ago: i64) -> UsageRecord {
        UsageRecord {
            id: None,
            timestamp: Utc::now() - chrono::Duration::days(days_ago),
            collector: "test".to_string(),
            tool: None,
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            input_tokens: 10,
            output_tokens: 5,
            reasoning_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            total_tokens: 15,
            cost_usd: 0.01,
            cost_cny: 0.07,
            latency_ms: None,
            is_stream: false,
            status_code: None,
            session_id: None,
            request_id: None,
            source_file: None,
            raw_json: None,
            notes: None,
        }
    }

    #[tokio::test]
    async fn maintenance_cycle_purges_stale_records() {
        let storage = Storage::memory().unwrap();
        let pricing = storage.load_pricing_engine().unwrap();
        storage.insert_record(&old_record(120)).unwrap();
        storage
            .set_data_config(&DataConfig { retention_days: 30 })
            .unwrap();

        let result = run_maintenance_cycle(&storage, &pricing).await.unwrap();
        assert_eq!(result.retention_days, 30);
        assert!(result.purged >= 1);
    }

    #[tokio::test]
    async fn maintenance_cycle_keeps_all_when_retention_zero() {
        let storage = Storage::memory().unwrap();
        let pricing = storage.load_pricing_engine().unwrap();
        storage.insert_record(&old_record(400)).unwrap();
        // Default DataConfig has retention_days = 0 (keep everything).
        let result = run_maintenance_cycle(&storage, &pricing).await.unwrap();
        assert_eq!(result.retention_days, 0);
        assert_eq!(result.purged, 0);
    }
}
