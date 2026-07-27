mod events;
mod scan;
mod ws;

pub use events::{event_bus, install_event_bus, notify_running_servers, EventBus, WsEvent};
pub use scan::{run_maintenance_cycle, run_scan, MaintenanceResult, ScanResult};

use alltokens_core::model::{CollectorsConfig, ClaudeQuotaResponse, CodexQuotaResponse, DataConfig, GeneralConfig, Pagination, PricingConfig, RequestFilter};
use alltokens_core::pricing::PricingEntry;
use alltokens_core::storage::Storage;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::response::Json;
use axum::routing::{get, post, put};
use axum::Router;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct AppState {
    storage: Storage,
    events: EventBus,
}

#[derive(Debug, Deserialize)]
struct StatsQuery {
    provider: Option<String>,
    model: Option<String>,
    collector: Option<String>,
    tool: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    last: Option<String>,
}

impl StatsQuery {
    fn to_filter(&self) -> RequestFilter {
        RequestFilter {
            provider: self.provider.clone(),
            model: self.model.clone(),
            collector: self.collector.clone(),
            tool: self.tool.clone(),
            start_date: self.start_date.clone().or_else(|| self.parse_last()),
            end_date: self.end_date.clone(),
            ..Default::default()
        }
    }
    fn parse_last(&self) -> Option<String> {
        self.last.as_ref().map(|s| {
            let days: i64 = s.trim_end_matches('d').parse().unwrap_or(7);
            let date = chrono::Utc::now() - chrono::Duration::days(days);
            date.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
        })
    }
}

#[derive(Debug, Deserialize)]
struct HeatmapQuery {
    provider: Option<String>,
    model: Option<String>,
    collector: Option<String>,
    tool: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    last: Option<String>,
    days: Option<u32>,
}

impl HeatmapQuery {
    fn to_filter(&self) -> RequestFilter {
        RequestFilter {
            provider: self.provider.clone(),
            model: self.model.clone(),
            collector: self.collector.clone(),
            tool: self.tool.clone(),
            start_date: self.start_date.clone().or_else(|| self.parse_last()),
            end_date: self.end_date.clone(),
            ..Default::default()
        }
    }

    fn parse_last(&self) -> Option<String> {
        self.last.as_ref().map(|s| {
            let days: i64 = s.trim_end_matches('d').parse().unwrap_or(7);
            let date = chrono::Utc::now() - chrono::Duration::days(days);
            date.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
        })
    }

    fn period_days(&self) -> u32 {
        if let Some(days) = self.days {
            return days;
        }
        self.last
            .as_ref()
            .map(|s| s.trim_end_matches('d').parse().unwrap_or(180))
            .unwrap_or(180)
    }
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    provider: Option<String>,
    model: Option<String>,
    collector: Option<String>,
    tool: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    last: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ExportQuery {
    format: String,
    provider: Option<String>,
    model: Option<String>,
    collector: Option<String>,
    tool: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    last: Option<String>,
}

impl ExportQuery {
    fn to_filter(&self) -> RequestFilter {
        RequestFilter {
            provider: self.provider.clone(),
            model: self.model.clone(),
            collector: self.collector.clone(),
            tool: self.tool.clone(),
            start_date: self.start_date.clone().or_else(|| self.parse_last()),
            end_date: self.end_date.clone(),
            ..Default::default()
        }
    }

    fn parse_last(&self) -> Option<String> {
        self.last.as_ref().map(|s| {
            let days: i64 = s.trim_end_matches('d').parse().unwrap_or(7);
            let date = chrono::Utc::now() - chrono::Duration::days(days);
            date.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
        })
    }
}

#[derive(Debug, Deserialize)]
struct ScanCompleteBody {
    total: usize,
}

#[derive(Debug, Serialize)]
struct ApiResponse<T: Serialize> {
    success: bool,
    data: T,
}

impl<T: Serialize> ApiResponse<T> {
    fn ok(data: T) -> Self {
        Self {
            success: true,
            data,
        }
    }
}

// --- MITM CA trust-store management (reuses alltokens_proxy::ca_install) ---

#[derive(Debug, Serialize)]
struct CaStatusPayload {
    /// "installed" | "not_installed" | "unknown"
    status: String,
    /// Whether the CA cert file has been generated on disk.
    cert_present: bool,
    /// Absolute path to the CA cert file.
    cert_path: String,
    /// "windows" | "macos" | "linux"
    platform: String,
}

/// Resolve the CA certificate path from the default proxy CA directory.
fn resolve_ca_cert_path() -> std::path::PathBuf {
    let dir = alltokens_proxy::ProxyConfig::default().ca_dir();
    alltokens_proxy::CertificateAuthority::cert_path(&dir)
}

/// Map the install status enum to a stable API string.
fn ca_status_label(status: alltokens_proxy::CaInstallStatus) -> &'static str {
    match status {
        alltokens_proxy::CaInstallStatus::Installed => "installed",
        alltokens_proxy::CaInstallStatus::NotInstalled => "not_installed",
        alltokens_proxy::CaInstallStatus::Unknown => "unknown",
    }
}

/// Current platform label for the trust-store backend.
fn platform_label() -> &'static str {
    match alltokens_proxy::TrustStore::detect() {
        alltokens_proxy::TrustStore::Windows => "windows",
        alltokens_proxy::TrustStore::MacOs => "macos",
        alltokens_proxy::TrustStore::Linux => "linux",
    }
}

/// Build a status payload by probing disk + trust store (blocking).
fn build_ca_status_payload() -> CaStatusPayload {
    let cert_path = resolve_ca_cert_path();
    let cert_present = cert_path.exists();
    let status = if cert_present {
        alltokens_proxy::status(&cert_path).unwrap_or(alltokens_proxy::CaInstallStatus::Unknown)
    } else {
        alltokens_proxy::CaInstallStatus::NotInstalled
    };
    CaStatusPayload {
        status: ca_status_label(status).to_string(),
        cert_present,
        cert_path: cert_path.display().to_string(),
        platform: platform_label().to_string(),
    }
}

async fn ca_status() -> Result<Json<ApiResponse<CaStatusPayload>>, StatusCode> {
    let payload = tokio::task::spawn_blocking(build_ca_status_payload)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ApiResponse::ok(payload)))
}

/// Request body for `POST /api/ca/install`.
///
/// Installing the MITM CA into the OS trust store is a sensitive, security-relevant
/// operation, so it is never performed implicitly: the caller must opt in with
/// `{"confirm": true}`. Any other body (or no body at all) yields a dry-run that
/// reports the planned action without touching the trust store.
#[derive(Debug, Default, Deserialize)]
struct CaInstallRequest {
    #[serde(default)]
    confirm: bool,
}

/// Response for `POST /api/ca/install`: the resulting trust-store status plus
/// whether the call was a dry-run and a human-readable summary of the action.
#[derive(Debug, Serialize)]
struct CaInstallPayload {
    /// True when no change was made because confirmation was absent.
    dry_run: bool,
    /// Human-readable description of the action taken (or that would be taken).
    action: String,
    #[serde(flatten)]
    status: CaStatusPayload,
}

async fn ca_install(
    body: Option<Json<CaInstallRequest>>,
) -> Result<Json<ApiResponse<CaInstallPayload>>, StatusCode> {
    let confirm = body.map(|Json(b)| b.confirm).unwrap_or(false);
    let payload = tokio::task::spawn_blocking(move || {
        if !confirm {
            // Dry-run: report the planned action without writing to the trust store.
            let status = build_ca_status_payload();
            let action = format!(
                "dry-run: would install CA into the {} trust store; re-send with {{\"confirm\": true}} to apply",
                status.platform
            );
            return Ok::<_, anyhow::Error>(CaInstallPayload {
                dry_run: true,
                action,
                status,
            });
        }
        let dir = alltokens_proxy::ProxyConfig::default().ca_dir();
        // Ensure the CA exists before installing.
        alltokens_proxy::CertificateAuthority::load_or_generate(&dir)?;
        let cert_path = alltokens_proxy::CertificateAuthority::cert_path(&dir);
        alltokens_proxy::install(&cert_path)?;
        Ok(CaInstallPayload {
            dry_run: false,
            action: "installed CA into system trust store".to_string(),
            status: build_ca_status_payload(),
        })
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ApiResponse::ok(payload)))
}

async fn ca_uninstall() -> Result<Json<ApiResponse<CaStatusPayload>>, StatusCode> {
    let payload = tokio::task::spawn_blocking(|| {
        let cert_path = resolve_ca_cert_path();
        alltokens_proxy::uninstall(&cert_path)?;
        Ok::<_, anyhow::Error>(build_ca_status_payload())
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ApiResponse::ok(payload)))
}

async fn overview(
    State(state): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<ApiResponse<alltokens_core::model::OverviewStats>>, StatusCode> {
    state
        .storage
        .get_overview(&q.to_filter())
        .map(|s| Json(ApiResponse::ok(s)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn providers(
    State(state): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<ApiResponse<Vec<alltokens_core::model::ProviderStats>>>, StatusCode> {
    state
        .storage
        .get_provider_stats(&q.to_filter())
        .map(|s| Json(ApiResponse::ok(s)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn models(
    State(state): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<ApiResponse<Vec<alltokens_core::model::ModelStats>>>, StatusCode> {
    state
        .storage
        .get_model_stats(&q.to_filter())
        .map(|s| Json(ApiResponse::ok(s)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn tools(
    State(state): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<ApiResponse<Vec<alltokens_core::model::ToolStats>>>, StatusCode> {
    state
        .storage
        .get_tool_stats(&q.to_filter())
        .map(|s| Json(ApiResponse::ok(s)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn projects(
    State(state): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<ApiResponse<Vec<alltokens_core::model::ProjectStats>>>, StatusCode> {
    state
        .storage
        .get_project_stats(&q.to_filter())
        .map(|s| Json(ApiResponse::ok(s)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn sessions(
    State(state): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<ApiResponse<Vec<alltokens_core::model::SessionStats>>>, StatusCode> {
    state
        .storage
        .get_session_stats(&q.to_filter())
        .map(|s| Json(ApiResponse::ok(s)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn tools_ranking(
    State(state): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<ApiResponse<Vec<alltokens_core::model::ToolInvocationStats>>>, StatusCode> {
    state
        .storage
        .get_tool_invocation_ranking(&q.to_filter())
        .map(|s| Json(ApiResponse::ok(s)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn skills_ranking(
    State(state): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<ApiResponse<Vec<alltokens_core::model::SkillInvocationStats>>>, StatusCode> {
    state
        .storage
        .get_skill_invocation_ranking(&q.to_filter())
        .map(|s| Json(ApiResponse::ok(s)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn trends(
    State(state): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<ApiResponse<Vec<alltokens_core::model::DailySummary>>>, StatusCode> {
    state
        .storage
        .get_daily_trends(&q.to_filter())
        .map(|s| Json(ApiResponse::ok(s)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn heatmap(
    State(state): State<AppState>,
    Query(q): Query<HeatmapQuery>,
) -> Result<Json<ApiResponse<alltokens_core::model::TokenHeatmap>>, StatusCode> {
    state
        .storage
        .get_token_heatmap(&q.to_filter(), q.period_days())
        .map(|s| Json(ApiResponse::ok(s)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn hour_of_week(
    State(state): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<ApiResponse<Vec<alltokens_core::model::HourOfWeekCell>>>, StatusCode> {
    state
        .storage
        .get_hour_of_week(&q.to_filter())
        .map(|s| Json(ApiResponse::ok(s)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn requests(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<
    Json<ApiResponse<alltokens_core::model::PaginatedResult<alltokens_core::model::UsageRecord>>>,
    StatusCode,
> {
    let filter = RequestFilter {
        provider: q.provider,
        model: q.model,
        collector: q.collector,
        tool: q.tool,
        start_date: q.start_date.or_else(|| {
            q.last.as_ref().map(|s| {
                let days: i64 = s.trim_end_matches('d').parse().unwrap_or(7);
                let date = chrono::Utc::now() - chrono::Duration::days(days);
                date.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
            })
        }),
        end_date: q.end_date,
        ..Default::default()
    };
    let pg = Pagination {
        page: q.page.unwrap_or(0),
        page_size: q.page_size.unwrap_or(50),
    };
    state
        .storage
        .get_requests(&filter, &pg)
        .map(|r| Json(ApiResponse::ok(r)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn export_data(
    State(state): State<AppState>,
    Query(q): Query<ExportQuery>,
) -> Result<Response, StatusCode> {
    let records = state
        .storage
        .export_requests(&q.to_filter())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let stamp = chrono::Utc::now().format("%Y%m%d");
    match q.format.as_str() {
        "csv" => {
            let body = alltokens_core::export::to_csv(&records);
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "text/csv; charset=utf-8".parse().unwrap());
            headers.insert(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"alltokens-{stamp}.csv\"")
                    .parse()
                    .unwrap(),
            );
            Ok((headers, body).into_response())
        }
        "json" => {
            let body = alltokens_core::export::to_json(&records)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                "application/json; charset=utf-8".parse().unwrap(),
            );
            headers.insert(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"alltokens-{stamp}.json\"")
                    .parse()
                    .unwrap(),
            );
            Ok((headers, body).into_response())
        }
        "pdf" => {
            // Print-ready HTML report; rendered inline so the browser can
            // auto-open the print dialog and "Save as PDF" (dependency-free).
            let body = alltokens_core::export::to_html_report(&records);
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                "text/html; charset=utf-8".parse().unwrap(),
            );
            Ok((headers, body).into_response())
        }
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

async fn scan(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<ScanResult>>, StatusCode> {
    let pricing = state
        .storage
        .load_pricing_engine()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    match run_scan(&state.storage, &pricing).await {
        Ok(result) => {
            state.events.emit_scan_complete(result.total);
            Ok(Json(ApiResponse::ok(result)))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn scan_complete_event(
    State(state): State<AppState>,
    Json(body): Json<ScanCompleteBody>,
) -> StatusCode {
    state.events.emit_scan_complete(body.total);
    StatusCode::NO_CONTENT
}

// --- Webhook ingest (Phase 4 Layer 3: tools push usage proactively) ---

/// 单次推送允许的最大记录数，防止误发超大批次打满磁盘。
const MAX_INGEST_RECORDS: usize = 1000;

/// Webhook 推送的单条 usage 记录。仅 `provider`/`model` 必填，其余缺省。
#[derive(Debug, Default, Clone, Deserialize)]
struct IngestRecord {
    provider: Option<String>,
    model: Option<String>,
    /// RFC3339；缺省 = 服务器当前时间
    timestamp: Option<String>,
    tool: Option<String>,
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    reasoning_tokens: u64,
    #[serde(default)]
    cache_read_tokens: u64,
    #[serde(default)]
    cache_creation_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
    /// 自带成本（USD）；为 0 时由定价表计算
    #[serde(default)]
    cost_usd: f64,
    latency_ms: Option<u64>,
    #[serde(default)]
    is_stream: bool,
    status_code: Option<u16>,
    session_id: Option<String>,
    request_id: Option<String>,
    notes: Option<String>,
    raw_json: Option<String>,
}

/// 接受两种形态：单条对象 或 `{"records": [...]}` 批量。
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum IngestBody {
    Batch { records: Vec<IngestRecord> },
    Single(IngestRecord),
}

#[derive(Debug, Serialize)]
struct IngestResult {
    inserted: usize,
    skipped: usize,
}

/// 归一化为 UsageRecord：collector 强制 "webhook"（不信任客户端自报来源），
/// total_tokens 缺省时求和四类 token。
fn ingest_record_to_usage(raw: IngestRecord) -> alltokens_core::model::UsageRecord {
    let timestamp = raw
        .timestamp
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);
    let mut record = alltokens_core::model::UsageRecord {
        id: None,
        timestamp,
        collector: "webhook".to_string(),
        tool: raw.tool,
        provider: raw.provider.unwrap_or_default(),
        model: raw.model.unwrap_or_default(),
        input_tokens: raw.input_tokens,
        output_tokens: raw.output_tokens,
        reasoning_tokens: raw.reasoning_tokens,
        cache_read_tokens: raw.cache_read_tokens,
        cache_creation_tokens: raw.cache_creation_tokens,
        total_tokens: raw.total_tokens,
        cost_usd: raw.cost_usd,
        cost_cny: 0.0,
        latency_ms: raw.latency_ms,
        is_stream: raw.is_stream,
        status_code: raw.status_code,
        session_id: raw.session_id,
        request_id: raw.request_id,
        source_file: None,
        raw_json: raw.raw_json,
        notes: raw.notes,
    };
    if record.total_tokens == 0 {
        record.total_tokens = record.input_tokens
            + record.output_tokens
            + record.cache_read_tokens
            + record.cache_creation_tokens;
    }
    record
}

async fn ingest(
    State(state): State<AppState>,
    Json(body): Json<IngestBody>,
) -> Result<Json<ApiResponse<IngestResult>>, StatusCode> {
    let raw_records = match body {
        IngestBody::Batch { records } => records,
        IngestBody::Single(record) => vec![record],
    };
    if raw_records.is_empty() || raw_records.len() > MAX_INGEST_RECORDS {
        return Err(StatusCode::BAD_REQUEST);
    }
    let pricing = state
        .storage
        .load_pricing_engine()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut records = Vec::with_capacity(raw_records.len());
    let mut skipped = 0usize;
    for raw in raw_records {
        let incomplete = raw.provider.as_deref().map(str::is_empty).unwrap_or(true)
            || raw.model.as_deref().map(str::is_empty).unwrap_or(true);
        if incomplete {
            skipped += 1;
            continue;
        }
        let mut record = ingest_record_to_usage(raw);
        pricing.calculate_cost(&mut record);
        records.push(record);
    }

    let inserted = if records.is_empty() {
        0
    } else {
        state
            .storage
            .insert_records(&records)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };
    if inserted > 0 {
        // 复用 scan_complete 事件通道，前端零改动自动刷新。
        state.events.emit_scan_complete(inserted);
    }
    Ok(Json(ApiResponse::ok(IngestResult { inserted, skipped })))
}

#[derive(Debug, Serialize)]
struct PricingSummary {
    model_count: usize,
    provider_count: usize,
    usd_to_cny: f64,
    overrides: Vec<PricingEntry>,
}

#[derive(Debug, Serialize)]
struct CollectorStatus {
    id: String,
    name: String,
    available: bool,
    enabled: bool,
    last_scan_at: Option<String>,
}

async fn get_pricing_config(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<PricingSummary>>, StatusCode> {
    let engine = state
        .storage
        .load_pricing_engine()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let config = state
        .storage
        .get_pricing_config()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ApiResponse::ok(PricingSummary {
        model_count: engine.model_count(),
        provider_count: engine.provider_count(),
        usd_to_cny: engine.usd_to_cny(),
        overrides: config.overrides,
    })))
}

async fn set_pricing_config(
    State(state): State<AppState>,
    Json(body): Json<PricingConfig>,
) -> Result<Json<ApiResponse<PricingSummary>>, StatusCode> {
    state
        .storage
        .set_pricing_config(&body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    get_pricing_config(State(state)).await
}

/// 列出全部定价条目（内置 + 用户覆盖），含 context_window，供前端计算上下文窗口占用 %。
async fn list_pricing_models(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<PricingEntry>>>, StatusCode> {
    let engine = state
        .storage
        .load_pricing_engine()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut models: Vec<PricingEntry> = engine.all_entries().into_iter().cloned().collect();
    models.sort_by(|a, b| {
        a.provider
            .cmp(&b.provider)
            .then_with(|| a.model.cmp(&b.model))
    });
    Ok(Json(ApiResponse::ok(models)))
}

async fn collectors_status(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<CollectorStatus>>>, StatusCode> {
    let collectors = alltokens_collectors::register_collectors();
    let config = state
        .storage
        .get_collectors_config()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut statuses = Vec::with_capacity(collectors.len());
    for collector in &collectors {
        let last_scan_at = state
            .storage
            .get_last_scan(collector.id())
            .ok()
            .flatten()
            .map(|dt| dt.to_rfc3339());
        let enabled = config
            .enabled
            .get(collector.id())
            .copied()
            .unwrap_or(true);
        statuses.push(CollectorStatus {
            id: collector.id().to_string(),
            name: collector.name().to_string(),
            available: collector.is_available(),
            enabled,
            last_scan_at,
        });
    }
    statuses.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(ApiResponse::ok(statuses)))
}

async fn set_collectors_config(
    State(state): State<AppState>,
    Json(body): Json<CollectorsConfig>,
) -> Result<Json<ApiResponse<Vec<CollectorStatus>>>, StatusCode> {
    state
        .storage
        .set_collectors_config(&body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    collectors_status(State(state)).await
}

async fn get_general_config(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<GeneralConfig>>, StatusCode> {
    state
        .storage
        .get_general_config()
        .map(|c| Json(ApiResponse::ok(c)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn set_general_config(
    State(state): State<AppState>,
    Json(body): Json<GeneralConfig>,
) -> Result<Json<ApiResponse<GeneralConfig>>, StatusCode> {
    state
        .storage
        .set_general_config(&body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ApiResponse::ok(body)))
}

async fn get_data_config(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<DataConfig>>, StatusCode> {
    state
        .storage
        .get_data_config()
        .map(|c| Json(ApiResponse::ok(c)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn set_data_config(
    State(state): State<AppState>,
    Json(body): Json<DataConfig>,
) -> Result<Json<ApiResponse<DataConfig>>, StatusCode> {
    state
        .storage
        .set_data_config(&body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if body.retention_days > 0 {
        let _ = state.storage.purge_records_older_than_days(body.retention_days);
    }
    Ok(Json(ApiResponse::ok(body)))
}

async fn get_budget_config(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<alltokens_core::model::BudgetConfig>>, StatusCode> {
    state
        .storage
        .get_budget_config()
        .map(|c| Json(ApiResponse::ok(c)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Debug, Deserialize)]
struct QuotaQuery {
    refresh: Option<bool>,
}

async fn codex_quota(
    State(state): State<AppState>,
    Query(q): Query<QuotaQuery>,
) -> Result<Json<ApiResponse<CodexQuotaResponse>>, StatusCode> {
    if q.refresh.unwrap_or(false) {
        match alltokens_collectors::codex_quota::fetch_codex_quota().await {
            Ok(snapshot) => {
                state
                    .storage
                    .set_codex_quota_snapshot(&snapshot)
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                return Ok(Json(ApiResponse::ok(CodexQuotaResponse {
                    snapshot: Some(snapshot),
                    error: None,
                })));
            }
            Err(e) => {
                let cached = state
                    .storage
                    .get_codex_quota_snapshot()
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                return Ok(Json(ApiResponse::ok(CodexQuotaResponse {
                    snapshot: cached,
                    error: Some(e.to_string()),
                })));
            }
        }
    }

    let snapshot = state
        .storage
        .get_codex_quota_snapshot()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ApiResponse::ok(CodexQuotaResponse {
        snapshot,
        error: None,
    })))
}

async fn claude_quota(
    State(state): State<AppState>,
    Query(q): Query<QuotaQuery>,
) -> Result<Json<ApiResponse<ClaudeQuotaResponse>>, StatusCode> {
    if q.refresh.unwrap_or(false) {
        match alltokens_collectors::claude_quota::fetch_claude_quota().await {
            Ok(snapshot) => {
                state
                    .storage
                    .set_claude_quota_snapshot(&snapshot)
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                return Ok(Json(ApiResponse::ok(ClaudeQuotaResponse {
                    snapshot: Some(snapshot),
                    error: None,
                })));
            }
            Err(e) => {
                let cached = state
                    .storage
                    .get_claude_quota_snapshot()
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                return Ok(Json(ApiResponse::ok(ClaudeQuotaResponse {
                    snapshot: cached,
                    error: Some(e.to_string()),
                })));
            }
        }
    }

    let snapshot = state
        .storage
        .get_claude_quota_snapshot()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ApiResponse::ok(ClaudeQuotaResponse {
        snapshot,
        error: None,
    })))
}

async fn set_budget_config(
    State(state): State<AppState>,
    Json(body): Json<alltokens_core::model::BudgetConfig>,
) -> Result<Json<ApiResponse<alltokens_core::model::BudgetConfig>>, StatusCode> {
    state
        .storage
        .set_budget_config(&body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ApiResponse::ok(body)))
}

async fn get_subscription_config(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<alltokens_core::model::SubscriptionConfig>>, StatusCode> {
    state
        .storage
        .get_subscription_config()
        .map(|c| Json(ApiResponse::ok(c)))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn set_subscription_config(
    State(state): State<AppState>,
    Json(body): Json<alltokens_core::model::SubscriptionConfig>,
) -> Result<Json<ApiResponse<alltokens_core::model::SubscriptionConfig>>, StatusCode> {
    state
        .storage
        .set_subscription_config(&body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ApiResponse::ok(body)))
}

pub struct WebConfig {
    pub listen_addr: std::net::SocketAddr,
    pub storage: Storage,
    pub static_dir: Option<std::path::PathBuf>,
    pub events: Option<EventBus>,
}

impl WebConfig {
    pub fn new(listen_addr: std::net::SocketAddr, storage: Storage) -> Self {
        Self {
            listen_addr,
            storage,
            static_dir: None,
            events: None,
        }
    }
    pub fn with_static(mut self, dir: std::path::PathBuf) -> Self {
        self.static_dir = Some(dir);
        self
    }
    pub fn with_events(mut self, events: EventBus) -> Self {
        self.events = Some(events);
        self
    }
}

/// Build the CORS layer restricted to the app's own origins.
///
/// The dashboard is served same-origin in `serve` mode, and the Tauri desktop
/// shell loads from the `tauri://localhost` / `http://tauri.localhost` origins
/// while calling the embedded API on loopback. Every other origin (e.g. a random
/// web page in the user's browser) is rejected, so sensitive endpoints such as
/// `POST /api/ca/install` cannot be driven cross-origin.
fn build_cors() -> tower_http::cors::CorsLayer {
    const ALLOWED_ORIGINS: [&str; 9] = [
        // Tauri desktop shell (scheme differs by platform / WebView).
        "tauri://localhost",
        "http://tauri.localhost",
        "https://tauri.localhost",
        // `alltokens serve` default port.
        "http://localhost:3210",
        "http://127.0.0.1:3210",
        // Tauri-embedded web server port.
        "http://localhost:3212",
        "http://127.0.0.1:3212",
        // Vite dev server (frontend dev; API is normally reached via its proxy).
        "http://localhost:5173",
        "http://127.0.0.1:5173",
    ];
    let origins: Vec<HeaderValue> = ALLOWED_ORIGINS
        .iter()
        .filter_map(|o| o.parse::<HeaderValue>().ok())
        .collect();
    tower_http::cors::CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE])
}

/// Assemble the full Axum app (API routes + restrictive CORS + optional static dir).
fn build_app(state: AppState, static_dir: Option<std::path::PathBuf>) -> Router {
    let api_routes = Router::new()
        .route("/health", get(health))
        .route("/overview", get(overview))
        .route("/providers", get(providers))
        .route("/models", get(models))
        .route("/tools", get(tools))
        .route("/tools/ranking", get(tools_ranking))
        .route("/skills/ranking", get(skills_ranking))
        .route("/projects", get(projects))
        .route("/sessions", get(sessions))
        .route("/trends", get(trends))
        .route("/heatmap", get(heatmap))
        .route("/heatmap/hourly", get(hour_of_week))
        .route("/requests", get(requests))
        .route("/export", get(export_data))
        .route("/scan", post(scan))
        .route("/events/scan-complete", post(scan_complete_event))
        .route("/ingest", post(ingest))
        .route("/config/budget", get(get_budget_config).put(set_budget_config))
        .route("/config/subscription", get(get_subscription_config).put(set_subscription_config))
        .route("/config/pricing", get(get_pricing_config).put(set_pricing_config))
        .route("/pricing/models", get(list_pricing_models))
        .route("/config/collectors", put(set_collectors_config))
        .route("/config/general", get(get_general_config).put(set_general_config))
        .route("/config/data", get(get_data_config).put(set_data_config))
        .route("/quota/codex", get(codex_quota))
        .route("/quota/claude", get(claude_quota))
        .route("/collectors", get(collectors_status))
        .route("/ca/status", get(ca_status))
        .route("/ca/install", post(ca_install))
        .route("/ca/uninstall", post(ca_uninstall))
        .route("/ws", get(ws::ws_handler));

    let mut app = Router::new()
        .nest("/api", api_routes)
        .layer(build_cors())
        .with_state(state);

    if let Some(static_dir) = static_dir {
        if static_dir.exists() {
            app = app.fallback_service(tower_http::services::ServeDir::new(static_dir));
        }
    }
    app
}

pub async fn start_web(config: WebConfig) -> anyhow::Result<()> {
    let events = config.events.unwrap_or_else(EventBus::new);
    install_event_bus(events.clone());

    let state = AppState {
        storage: config.storage,
        events,
    };

    let auto_scan_state = state.clone();
    let app = build_app(state, config.static_dir);

    tracing::info!("Starting web server on {}", config.listen_addr);
    let listener = tokio::net::TcpListener::bind(config.listen_addr).await?;

    tokio::spawn(async move {
        start_auto_scan_loop(auto_scan_state).await;
    });

    axum::serve(listener, app).await?;
    Ok(())
}

async fn start_auto_scan_loop(state: AppState) {
    loop {
        let interval_mins = state
            .storage
            .get_general_config()
            .map(|c| c.auto_scan_interval_minutes)
            .unwrap_or(0);

        if interval_mins == 0 {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            continue;
        }

        tokio::time::sleep(std::time::Duration::from_secs(interval_mins as u64 * 60)).await;

        let pricing = match state.storage.load_pricing_engine() {
            Ok(engine) => engine,
            Err(e) => {
                tracing::warn!("Auto-scan: failed to load pricing: {e}");
                continue;
            }
        };

        match run_scan(&state.storage, &pricing).await {
            Ok(result) => {
                tracing::info!("Auto-scan complete: {} records", result.total);
                state.events.emit_scan_complete(result.total);
            }
            Err(e) => tracing::warn!("Auto-scan failed: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_event_serializes_with_type_tag() {
        let event = WsEvent::ScanComplete { total: 42 };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"scan_complete""#));
        assert!(json.contains(r#""total":42"#));
    }

    #[tokio::test]
    async fn event_bus_delivers_scan_complete() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        bus.emit_scan_complete(7);
        let event = rx.recv().await.unwrap();
        assert!(matches!(event, WsEvent::ScanComplete { total: 7 }));
    }

    #[test]
    fn ca_status_label_mapping() {
        assert_eq!(
            ca_status_label(alltokens_proxy::CaInstallStatus::Installed),
            "installed"
        );
        assert_eq!(
            ca_status_label(alltokens_proxy::CaInstallStatus::NotInstalled),
            "not_installed"
        );
        assert_eq!(
            ca_status_label(alltokens_proxy::CaInstallStatus::Unknown),
            "unknown"
        );
    }

    #[test]
    fn resolve_ca_cert_path_ends_with_cert() {
        let path = resolve_ca_cert_path();
        assert!(path.ends_with("alltokens-ca.crt"));
    }

    #[test]
    fn platform_label_matches_target() {
        let label = platform_label();
        #[cfg(target_os = "windows")]
        assert_eq!(label, "windows");
        #[cfg(target_os = "macos")]
        assert_eq!(label, "macos");
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        assert_eq!(label, "linux");
    }

    #[tokio::test]
    async fn cors_allows_app_origin_and_rejects_foreign() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let app = build_app(test_app_state(), None);

        // Foreign origin preflight → no Access-Control-Allow-Origin echoed back,
        // so the browser blocks the actual (sensitive) request.
        let evil = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/api/ca/install")
                    .header(header::ORIGIN, "https://evil.example.com")
                    .header("access-control-request-method", "POST")
                    .header("access-control-request-headers", "content-type")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            evil.headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none(),
            "foreign origin must not receive an Access-Control-Allow-Origin header"
        );

        // App (Tauri) origin preflight → Access-Control-Allow-Origin echoes it back.
        let allowed = app
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/api/ca/install")
                    .header(header::ORIGIN, "http://tauri.localhost")
                    .header("access-control-request-method", "POST")
                    .header("access-control-request-headers", "content-type")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            allowed
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|v| v.to_str().ok()),
            Some("http://tauri.localhost"),
            "app origin must be allowed through CORS"
        );
    }

    #[tokio::test]
    async fn ca_install_without_confirm_is_dry_run() {
        // No confirmation → dry-run: the response is flagged and no install is attempted.
        let Json(resp) = ca_install(Some(Json(CaInstallRequest { confirm: false })))
            .await
            .unwrap();
        assert!(resp.data.dry_run, "unconfirmed install must be a dry-run");
        assert!(resp.data.action.contains("dry-run"));
    }

    #[tokio::test]
    async fn ca_install_missing_body_defaults_to_dry_run() {
        // A bare POST with no body must never silently write to the trust store.
        let Json(resp) = ca_install(None).await.unwrap();
        assert!(resp.data.dry_run);
    }

    #[tokio::test]
    async fn scan_smoke_completes_without_error() {
        use alltokens_core::storage::Storage;

        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("scan-smoke.db");
        let storage = Storage::open(&db).unwrap();
        let pricing = storage.load_pricing_engine().unwrap();
        let result = run_scan(&storage, &pricing).await.unwrap();
        // Scans all registered collectors; count varies by installed tools
        assert!(result.by_collector.len() <= 24);
    }

    #[tokio::test]
    async fn pricing_config_round_trip() {
        use alltokens_core::model::PricingConfig;
        use alltokens_core::pricing::PricingEntry;
        use alltokens_core::storage::Storage;

        let storage = Storage::memory().unwrap();
        let config = PricingConfig {
            usd_to_cny: Some(7.5),
            overrides: vec![PricingEntry {
                provider: "openai".to_string(),
                model: "gpt-4o".to_string(),
                input_per_mtok: 1.0,
                output_per_mtok: 2.0,
                cache_read_per_mtok: 0.0,
                cache_create_per_mtok: 0.0,
                context_window: 0,
            }],
        };
        storage.set_pricing_config(&config).unwrap();
        let engine = storage.load_pricing_engine().unwrap();
        assert!((engine.usd_to_cny() - 7.5).abs() < f64::EPSILON);
        let entry = engine.find("openai", "gpt-4o").unwrap();
        assert!((entry.input_per_mtok - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn pricing_models_include_context_window() {
        use alltokens_core::storage::Storage;

        // 镜像 list_pricing_models 的数据路径：加载引擎 → all_entries → 排序。
        let storage = Storage::memory().unwrap();
        let engine = storage.load_pricing_engine().unwrap();
        let mut models: Vec<PricingEntry> = engine.all_entries().into_iter().cloned().collect();
        models.sort_by(|a, b| a.provider.cmp(&b.provider).then_with(|| a.model.cmp(&b.model)));
        // 内置表已为主流模型补齐 context_window
        let gpt4o = models
            .iter()
            .find(|e| e.provider == "openai" && e.model == "gpt-4o")
            .unwrap();
        assert_eq!(gpt4o.context_window, 128000);
        // provider 升序排序稳定
        assert!(models.windows(2).all(|w| w[0].provider <= w[1].provider));
    }

    #[tokio::test]
    async fn budget_config_api_round_trip() {
        use alltokens_core::model::BudgetConfig;
        use alltokens_core::storage::Storage;

        let storage = Storage::memory().unwrap();
        let config = BudgetConfig {
            monthly_usd: Some(50.0),
            enabled: true,
        };
        storage.set_budget_config(&config).unwrap();
        let loaded = storage.get_budget_config().unwrap();
        assert_eq!(loaded.monthly_usd, Some(50.0));
        assert!(loaded.enabled);
    }

    #[tokio::test]
    async fn codex_quota_snapshot_round_trip() {
        use alltokens_core::model::{
            CodexQuotaSnapshot, CodexQuotaWindow, CodexQuotaWindowKind,
        };
        use alltokens_core::storage::Storage;
        use chrono::Utc;

        let storage = Storage::memory().unwrap();
        let snapshot = CodexQuotaSnapshot {
            fetched_at: Utc::now(),
            source: "test".to_string(),
            plan_type: Some("plus".to_string()),
            five_hour: Some(CodexQuotaWindow {
                kind: CodexQuotaWindowKind::FiveHour,
                used_percent: Some(25),
                remaining_percent: Some(75),
                window_duration_mins: Some(300),
                resets_at: Some(1_779_459_394),
            }),
            seven_day: Some(CodexQuotaWindow {
                kind: CodexQuotaWindowKind::SevenDay,
                used_percent: Some(18),
                remaining_percent: Some(82),
                window_duration_mins: Some(10_080),
                resets_at: Some(1_779_826_837),
            }),
            rate_limit_reached: false,
        };
        storage.set_codex_quota_snapshot(&snapshot).unwrap();
        let loaded = storage.get_codex_quota_snapshot().unwrap().unwrap();
        assert_eq!(loaded.plan_type, snapshot.plan_type);
        assert_eq!(loaded.five_hour, snapshot.five_hour);
        assert_eq!(loaded.seven_day, snapshot.seven_day);
    }

    #[tokio::test]
    async fn claude_quota_snapshot_round_trip() {
        use alltokens_core::model::{
            ClaudeQuotaSnapshot, CodexQuotaWindow, CodexQuotaWindowKind,
        };
        use alltokens_core::storage::Storage;
        use chrono::Utc;

        let storage = Storage::memory().unwrap();
        let snapshot = ClaudeQuotaSnapshot {
            fetched_at: Utc::now(),
            source: "test".to_string(),
            snapshot_path: Some("/tmp/statusline-snapshot.json".to_string()),
            captured_at: Some(Utc::now()),
            is_stale: false,
            five_hour: Some(CodexQuotaWindow {
                kind: CodexQuotaWindowKind::FiveHour,
                used_percent: Some(30),
                remaining_percent: Some(70),
                window_duration_mins: Some(300),
                resets_at: Some(1_779_459_394),
            }),
            seven_day: Some(CodexQuotaWindow {
                kind: CodexQuotaWindowKind::SevenDay,
                used_percent: Some(12),
                remaining_percent: Some(88),
                window_duration_mins: Some(10_080),
                resets_at: Some(1_779_826_837),
            }),
        };
        storage.set_claude_quota_snapshot(&snapshot).unwrap();
        let loaded = storage.get_claude_quota_snapshot().unwrap().unwrap();
        assert_eq!(loaded.snapshot_path, snapshot.snapshot_path);
        assert_eq!(loaded.five_hour, snapshot.five_hour);
        assert_eq!(loaded.seven_day, snapshot.seven_day);
        assert!(!loaded.is_stale);
    }

    #[test]
    fn session_stats_grouped_via_storage() {
        use alltokens_core::model::{RequestFilter, UsageRecord};
        use alltokens_core::storage::Storage;
        use chrono::{DateTime, Utc};

        // 镜像 sessions handler 的数据路径：插入带 session_id 的记录 → get_session_stats。
        let storage = Storage::memory().unwrap();
        let make = |ts: &str, sid: Option<&str>, tokens: u64| UsageRecord {
            id: None,
            timestamp: DateTime::parse_from_rfc3339(ts).unwrap().with_timezone(&Utc),
            collector: "codex".to_string(),
            tool: Some("Codex".to_string()),
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            input_tokens: tokens,
            output_tokens: 0,
            reasoning_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            total_tokens: tokens,
            cost_usd: 0.001,
            cost_cny: 0.007,
            latency_ms: Some(100),
            is_stream: false,
            status_code: Some(200),
            session_id: sid.map(|s| s.to_string()),
            request_id: None,
            source_file: None,
            raw_json: None,
            notes: None,
        };
        storage
            .insert_records(&[
                make("2026-07-10T10:00:00Z", Some("sess-x"), 100),
                make("2026-07-10T10:10:00Z", Some("sess-x"), 200),
                make("2026-07-10T11:00:00Z", None, 999),
            ])
            .unwrap();

        let stats = storage.get_session_stats(&RequestFilter::default()).unwrap();
        assert_eq!(stats.len(), 1, "orphan record without session_id is excluded");
        assert_eq!(stats[0].session_id, "sess-x");
        assert_eq!(stats[0].request_count, 2);
        assert_eq!(stats[0].total_tokens, 300);
        assert_eq!(stats[0].duration_secs, 600);
    }

    #[test]
    fn hour_of_week_aggregation_matches_endpoint_path() {
        use alltokens_core::model::{RequestFilter, UsageRecord};
        use alltokens_core::storage::Storage;
        use chrono::{DateTime, Utc};

        // 镜像 hour_of_week handler 的数据路径：插入记录 → get_hour_of_week。
        let storage = Storage::memory().unwrap();
        let make = |ts: &str, tokens: u64| UsageRecord {
            id: None,
            timestamp: DateTime::parse_from_rfc3339(ts).unwrap().with_timezone(&Utc),
            collector: "codex".to_string(),
            tool: Some("Codex".to_string()),
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            input_tokens: tokens,
            output_tokens: 0,
            reasoning_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            total_tokens: tokens,
            cost_usd: 0.001,
            cost_cny: 0.007,
            latency_ms: Some(100),
            is_stream: false,
            status_code: Some(200),
            session_id: None,
            request_id: None,
            source_file: None,
            raw_json: None,
            notes: None,
        };
        storage
            .insert_records(&[
                make("2026-07-14T10:00:00+00:00", 100),
                make("2026-07-14T10:30:00+00:00", 200),
            ])
            .unwrap();

        let cells = storage.get_hour_of_week(&RequestFilter::default()).unwrap();
        assert!(cells.iter().all(|c| c.weekday <= 6 && c.hour <= 23));
        let total_requests: u64 = cells.iter().map(|c| c.request_count).sum();
        assert_eq!(total_requests, 2);
        let total_tokens: u64 = cells.iter().map(|c| c.total_tokens).sum();
        assert_eq!(total_tokens, 300);
    }

    fn test_app_state() -> AppState {
        AppState {
            storage: alltokens_core::storage::Storage::memory().unwrap(),
            events: EventBus::new(),
        }
    }

    #[tokio::test]
    async fn ingest_single_record_computes_cost_and_forces_collector() {
        use alltokens_core::model::{Pagination, RequestFilter};

        let state = test_app_state();
        let storage = state.storage.clone();
        let body = IngestBody::Single(IngestRecord {
            provider: Some("openai".to_string()),
            model: Some("gpt-4o".to_string()),
            input_tokens: 1_000_000,
            output_tokens: 100_000,
            ..Default::default()
        });
        let Json(resp) = ingest(State(state), Json(body)).await.unwrap();
        assert_eq!(resp.data.inserted, 1);
        assert_eq!(resp.data.skipped, 0);

        let page = storage
            .get_requests(&RequestFilter::default(), &Pagination { page: 0, page_size: 10 })
            .unwrap();
        assert_eq!(page.total, 1);
        let record = &page.items[0];
        assert_eq!(record.collector, "webhook");
        // 定价表: gpt-4o input $2.5/M + output $10/M → 2.5 + 1.0 = 3.5
        assert!((record.cost_usd - 3.5).abs() < 1e-9);
        assert!(record.cost_cny > 0.0);
    }

    #[tokio::test]
    async fn ingest_batch_sums_total_and_preserves_given_cost() {
        use alltokens_core::model::{Pagination, RequestFilter};

        let state = test_app_state();
        let storage = state.storage.clone();
        let body = IngestBody::Batch {
            records: vec![
                // total 缺省 → input + output + cache_read + cache_creation
                IngestRecord {
                    provider: Some("anthropic".to_string()),
                    model: Some("claude-sonnet-4-20250514".to_string()),
                    input_tokens: 100,
                    output_tokens: 50,
                    cache_read_tokens: 30,
                    cache_creation_tokens: 20,
                    ..Default::default()
                },
                // 自带成本（未知模型，定价表无条目）→ 保留并换算 CNY
                IngestRecord {
                    provider: Some("custom".to_string()),
                    model: Some("my-model".to_string()),
                    input_tokens: 10,
                    total_tokens: 10,
                    cost_usd: 1.23,
                    ..Default::default()
                },
            ],
        };
        let Json(resp) = ingest(State(state), Json(body)).await.unwrap();
        assert_eq!(resp.data.inserted, 2);

        let page = storage
            .get_requests(&RequestFilter::default(), &Pagination { page: 0, page_size: 10 })
            .unwrap();
        let claude = page.items.iter().find(|r| r.provider == "anthropic").unwrap();
        assert_eq!(claude.total_tokens, 200);
        let custom = page.items.iter().find(|r| r.provider == "custom").unwrap();
        assert!((custom.cost_usd - 1.23).abs() < 1e-9);
        assert!(custom.cost_cny > 0.0);
    }

    #[tokio::test]
    async fn ingest_rejects_over_limit_and_skips_incomplete() {
        use alltokens_core::model::{Pagination, RequestFilter};

        // 超过批次上限 → 400
        let state = test_app_state();
        let too_many = vec![IngestRecord::default(); MAX_INGEST_RECORDS + 1];
        let err = ingest(State(state), Json(IngestBody::Batch { records: too_many }))
            .await
            .unwrap_err();
        assert_eq!(err, StatusCode::BAD_REQUEST);

        // 缺 provider/model 的条目跳过计数，合法条目正常入库
        let state = test_app_state();
        let storage = state.storage.clone();
        let body = IngestBody::Batch {
            records: vec![
                IngestRecord {
                    provider: Some("openai".to_string()),
                    model: Some("gpt-4o".to_string()),
                    input_tokens: 5,
                    ..Default::default()
                },
                IngestRecord {
                    provider: Some("openai".to_string()),
                    model: None,
                    ..Default::default()
                },
                IngestRecord::default(),
            ],
        };
        let Json(resp) = ingest(State(state), Json(body)).await.unwrap();
        assert_eq!(resp.data.inserted, 1);
        assert_eq!(resp.data.skipped, 2);
        let page = storage
            .get_requests(&RequestFilter::default(), &Pagination { page: 0, page_size: 10 })
            .unwrap();
        assert_eq!(page.total, 1);
    }
}
