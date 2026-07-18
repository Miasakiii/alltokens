use crate::model::HeatmapDay;
use chrono::{Duration, NaiveDate};

/// Default heatmap window (~6 months), matching codexU half-year pattern.
pub const DEFAULT_HEATMAP_DAYS: u32 = 180;

/// Inclusive date range for a heatmap period ending on `end`.
pub fn heatmap_date_range(period_days: u32, end: NaiveDate) -> (NaiveDate, NaiveDate) {
    let span = period_days.max(1) as i64;
    let start = end - Duration::days(span - 1);
    (start, end)
}

/// Merge SQL aggregates into a dense per-day series (zero-fill missing dates).
pub fn fill_heatmap_days(
    aggregated: &[(String, u64, f64, f64, u64)],
    start: NaiveDate,
    end: NaiveDate,
) -> Vec<HeatmapDay> {
    let mut by_date: std::collections::HashMap<&str, (u64, f64, f64, u64)> =
        std::collections::HashMap::new();
    for (date, tokens, cost_usd, cost_cny, count) in aggregated {
        by_date.insert(
            date.as_str(),
            (*tokens, *cost_usd, *cost_cny, *count),
        );
    }

    let mut days = Vec::new();
    let mut cursor = start;
    while cursor <= end {
        let key = cursor.format("%Y-%m-%d").to_string();
        let (total_tokens, total_cost_usd, total_cost_cny, request_count) = by_date
            .get(key.as_str())
            .copied()
            .unwrap_or((0, 0.0, 0.0, 0));
        days.push(HeatmapDay {
            date: key,
            total_tokens,
            total_cost_usd,
            total_cost_cny,
            request_count,
        });
        cursor += Duration::days(1);
    }
    days
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn heatmap_date_range_spans_inclusive_days() {
        let end = NaiveDate::from_ymd_opt(2026, 7, 13).unwrap();
        let (start, end_out) = heatmap_date_range(7, end);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 7, 7).unwrap());
        assert_eq!(end_out, end);
    }

    #[test]
    fn fill_heatmap_days_zero_fills_gaps() {
        let start = NaiveDate::from_ymd_opt(2026, 7, 10).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 7, 12).unwrap();
        let aggregated = vec![
            ("2026-07-10".to_string(), 1000, 0.01, 0.07, 2),
            ("2026-07-12".to_string(), 500, 0.005, 0.035, 1),
        ];

        let days = fill_heatmap_days(&aggregated, start, end);
        assert_eq!(days.len(), 3);
        assert_eq!(days[0].date, "2026-07-10");
        assert_eq!(days[0].total_tokens, 1000);
        assert_eq!(days[1].date, "2026-07-11");
        assert_eq!(days[1].total_tokens, 0);
        assert_eq!(days[2].date, "2026-07-12");
        assert_eq!(days[2].total_tokens, 500);
    }

    #[test]
    fn fill_heatmap_days_aggregates_are_not_merged_here() {
        let start = NaiveDate::from_ymd_opt(2026, 7, 10).unwrap();
        let end = NaiveDate::from_ymd_opt(2026, 7, 10).unwrap();
        let aggregated = vec![("2026-07-10".to_string(), 3000, 0.03, 0.21, 3)];
        let days = fill_heatmap_days(&aggregated, start, end);
        assert_eq!(days[0].total_tokens, 3000);
        assert!((days[0].total_cost_usd - 0.03).abs() < f64::EPSILON);
    }
}
