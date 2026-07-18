use crate::model::UsageRecord;

const CSV_HEADERS: &[&str] = &[
    "timestamp",
    "collector",
    "tool",
    "provider",
    "model",
    "input_tokens",
    "output_tokens",
    "cache_read_tokens",
    "cache_creation_tokens",
    "total_tokens",
    "cost_usd",
    "cost_cny",
    "latency_ms",
    "is_stream",
    "status_code",
    "session_id",
    "request_id",
    "source_file",
    "notes",
];

/// Serialize usage records as pretty-printed JSON.
pub fn to_json(records: &[UsageRecord]) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(records)
}

/// Serialize usage records as CSV with a header row.
pub fn to_csv(records: &[UsageRecord]) -> String {
    let mut out = String::new();
    out.push_str(&CSV_HEADERS.join(","));
    out.push('\n');
    for record in records {
        out.push_str(&csv_row(record));
        out.push('\n');
    }
    out
}

fn csv_row(r: &UsageRecord) -> String {
    [
        escape_csv(&r.timestamp.to_rfc3339()),
        escape_csv(&r.collector),
        escape_csv(opt_str(&r.tool)),
        escape_csv(&r.provider),
        escape_csv(&r.model),
        r.input_tokens.to_string(),
        r.output_tokens.to_string(),
        r.cache_read_tokens.to_string(),
        r.cache_creation_tokens.to_string(),
        r.total_tokens.to_string(),
        r.cost_usd.to_string(),
        r.cost_cny.to_string(),
        opt_u64(r.latency_ms),
        r.is_stream.to_string(),
        opt_u16(r.status_code),
        escape_csv(opt_str(&r.session_id)),
        escape_csv(opt_str(&r.request_id)),
        escape_csv(opt_str(&r.source_file)),
        escape_csv(opt_str(&r.notes)),
    ]
    .join(",")
}

fn escape_csv(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn opt_str(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("")
}

fn opt_u64(value: Option<u64>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

fn opt_u16(value: Option<u16>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

/// Per-group accumulator for the HTML report tables.
#[derive(Default)]
struct RowAgg {
    requests: u64,
    tokens: u64,
    cost_usd: f64,
}

impl RowAgg {
    fn add(&mut self, r: &UsageRecord) {
        self.requests += 1;
        self.tokens += r.total_tokens;
        self.cost_usd += r.cost_usd;
    }
}

const REPORT_CSS: &str = r#"
  *{box-sizing:border-box}
  body{font-family:-apple-system,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;color:#1e293b;margin:0;padding:32px;background:#fff}
  h1{font-size:22px;margin:0 0 2px}
  h2{font-size:15px;margin:26px 0 8px;color:#334155;border-bottom:2px solid #e2e8f0;padding-bottom:4px}
  .meta{color:#64748b;font-size:12px;margin:0 0 18px}
  .cards{display:flex;flex-wrap:wrap;gap:12px;margin-bottom:6px}
  .card{flex:1 1 140px;border:1px solid #e2e8f0;border-radius:10px;padding:12px 14px;background:#f8fafc}
  .card .k{font-size:11px;color:#64748b;text-transform:uppercase;letter-spacing:.04em}
  .card .v{font-size:18px;font-weight:600;color:#0f172a;margin-top:4px}
  table{width:100%;border-collapse:collapse;font-size:12px}
  th,td{text-align:left;padding:6px 8px;border-bottom:1px solid #eef2f7}
  th{color:#64748b;font-weight:600;background:#f8fafc}
  td.num,th.num{text-align:right;font-variant-numeric:tabular-nums}
  .foot{margin-top:26px;color:#94a3b8;font-size:11px}
  @page{size:A4;margin:16mm}
  @media print{body{padding:0}.no-print{display:none}}
"#;

fn fmt_int(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

fn fmt_usd(v: f64) -> String {
    format!("${v:.4}")
}

fn fmt_cny(v: f64) -> String {
    format!("¥{v:.2}")
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Render usage records as a standalone, print-ready HTML report.
///
/// Opened in a browser the document auto-invokes the print dialog, letting the
/// user "Save as PDF" — a dependency-free PDF export path shared by the CLI
/// (`export --format pdf`) and the Web API (`/api/export?format=pdf`).
pub fn to_html_report(records: &[UsageRecord]) -> String {
    let mut total_input = 0u64;
    let mut total_output = 0u64;
    let mut total_tokens = 0u64;
    let mut total_cost_usd = 0.0f64;
    let mut total_cost_cny = 0.0f64;
    let mut by_provider: std::collections::BTreeMap<String, RowAgg> = std::collections::BTreeMap::new();
    let mut by_model: std::collections::BTreeMap<String, RowAgg> = std::collections::BTreeMap::new();
    let mut first_ts: Option<chrono::DateTime<chrono::Utc>> = None;
    let mut last_ts: Option<chrono::DateTime<chrono::Utc>> = None;

    for r in records {
        total_input += r.input_tokens;
        total_output += r.output_tokens;
        total_tokens += r.total_tokens;
        total_cost_usd += r.cost_usd;
        total_cost_cny += r.cost_cny;
        by_provider.entry(r.provider.clone()).or_default().add(r);
        by_model.entry(r.model.clone()).or_default().add(r);
        first_ts = Some(first_ts.map_or(r.timestamp, |t| t.min(r.timestamp)));
        last_ts = Some(last_ts.map_or(r.timestamp, |t| t.max(r.timestamp)));
    }

    let generated = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let range = match (first_ts, last_ts) {
        (Some(a), Some(b)) => format!(
            "{} → {}",
            a.format("%Y-%m-%d %H:%M"),
            b.format("%Y-%m-%d %H:%M")
        ),
        _ => "—".to_string(),
    };

    let cmp_cost = |a: &(String, RowAgg), b: &(String, RowAgg)| {
        b.1.cost_usd
            .partial_cmp(&a.1.cost_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
    };
    let mut providers: Vec<(String, RowAgg)> = by_provider.into_iter().collect();
    providers.sort_by(cmp_cost);
    let mut models: Vec<(String, RowAgg)> = by_model.into_iter().collect();
    models.sort_by(cmp_cost);
    let model_shown = models.len().min(20);

    let row = |name: &str, agg: &RowAgg| {
        format!(
            "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
            escape_html(name),
            fmt_int(agg.requests),
            fmt_int(agg.tokens),
            fmt_usd(agg.cost_usd)
        )
    };
    let provider_rows: String = providers.iter().map(|(n, a)| row(n, a)).collect();
    let model_rows: String = models.iter().take(model_shown).map(|(n, a)| row(n, a)).collect();

    format!(
        r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"><title>AllTokens Usage Report</title><style>{css}</style></head>
<body>
<h1>AllTokens Usage Report</h1>
<p class="meta">Generated {generated} · Range {range} · {reqs} requests</p>
<div class="cards">
  <div class="card"><div class="k">Requests</div><div class="v">{reqs}</div></div>
  <div class="card"><div class="k">Total tokens</div><div class="v">{total}</div></div>
  <div class="card"><div class="k">Input / Output</div><div class="v">{input} / {output}</div></div>
  <div class="card"><div class="k">Cost</div><div class="v">{cost_usd} · {cost_cny}</div></div>
</div>
<h2>By provider</h2>
<table><thead><tr><th>Provider</th><th class="num">Reqs</th><th class="num">Tokens</th><th class="num">Cost</th></tr></thead><tbody>{provider_rows}</tbody></table>
<h2>By model (top {model_shown})</h2>
<table><thead><tr><th>Model</th><th class="num">Reqs</th><th class="num">Tokens</th><th class="num">Cost</th></tr></thead><tbody>{model_rows}</tbody></table>
<p class="foot">AllTokens · dependency-free PDF export — use your browser's “Save as PDF”.</p>
<script>window.addEventListener('load',function(){{setTimeout(function(){{window.print();}},300);}});</script>
</body></html>"#,
        css = REPORT_CSS,
        generated = generated,
        range = range,
        reqs = fmt_int(records.len() as u64),
        total = fmt_int(total_tokens),
        input = fmt_int(total_input),
        output = fmt_int(total_output),
        cost_usd = fmt_usd(total_cost_usd),
        cost_cny = fmt_cny(total_cost_cny),
        provider_rows = provider_rows,
        model_shown = model_shown,
        model_rows = model_rows,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn sample_record() -> UsageRecord {
        UsageRecord {
            id: Some(1),
            timestamp: Utc.with_ymd_and_hms(2026, 7, 12, 10, 0, 0).unwrap(),
            collector: "claude_code".into(),
            tool: Some("Claude Code".into()),
            provider: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            input_tokens: 100,
            output_tokens: 50,
            reasoning_tokens: 0,
            cache_read_tokens: 10,
            cache_creation_tokens: 5,
            total_tokens: 165,
            cost_usd: 0.0012,
            cost_cny: 0.0086,
            latency_ms: Some(420),
            is_stream: true,
            status_code: Some(200),
            session_id: Some("sess-1".into()),
            request_id: Some("req-1".into()),
            source_file: None,
            raw_json: None,
            notes: Some("test, \"quoted\"".into()),
        }
    }

    #[test]
    fn json_roundtrip() {
        let records = vec![sample_record()];
        let json = to_json(&records).unwrap();
        let parsed: Vec<UsageRecord> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn csv_has_header_and_escapes_fields() {
        let csv = to_csv(&[sample_record()]);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("timestamp,collector"));
        assert!(lines[1].contains("\"test, \"\"quoted\"\"\""));
    }

    #[test]
    fn html_report_has_summary_and_escapes() {
        let mut r = sample_record();
        r.provider = "ev<il>".into();
        let html = to_html_report(&[r]);
        // document + auto-print script present
        assert!(html.contains("AllTokens Usage Report"));
        assert!(html.contains("window.print()"));
        // total_tokens summary (165) rendered
        assert!(html.contains("165"));
        // provider name is HTML-escaped, raw markup not injected
        assert!(html.contains("ev&lt;il&gt;"));
        assert!(!html.contains("ev<il>"));
    }
}
