use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 一次 API 调用的完整记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: Option<i64>,
    /// 请求时间 (ISO 8601)
    pub timestamp: DateTime<Utc>,
    /// 数据来源: 'claude_code' | 'cursor' | 'proxy' | 'cc_switch' | ...
    pub collector: String,
    /// 工具名: 'Claude Code' | 'Cursor' | 'Codex' | ...
    pub tool: Option<String>,
    /// Provider: 'openai' | 'anthropic' | 'deepseek' | ...
    pub provider: String,
    /// 模型: 'gpt-4o' | 'claude-sonnet-4-20250514' | ...
    pub model: String,

    // ── Token 用量 ──
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// 推理 token (o1/o3 等推理模型的 reasoning_output_tokens)
    pub reasoning_tokens: u64,
    /// 缓存命中 (prompt caching read)
    pub cache_read_tokens: u64,
    /// 缓存创建 (prompt caching write)
    pub cache_creation_tokens: u64,
    /// 总 token (冗余但查询方便)
    pub total_tokens: u64,

    // ── 成本 ──
    pub cost_usd: f64,
    pub cost_cny: f64,

    // ── 元数据 ──
    /// 请求延迟 (ms)
    pub latency_ms: Option<u64>,
    /// 是否流式
    pub is_stream: bool,
    /// HTTP 状态码
    pub status_code: Option<u16>,
    /// 会话/项目标识
    pub session_id: Option<String>,
    /// API 返回的 request id
    pub request_id: Option<String>,

    // ── 上下文 ──
    /// 日志来源文件路径
    pub source_file: Option<String>,
    /// 原始 JSON (可选保留, 调试用)
    pub raw_json: Option<String>,
    /// 用户备注
    pub notes: Option<String>,
}

impl UsageRecord {
    /// 计算总 token
    pub fn compute_total(&mut self) {
        self.total_tokens =
            self.input_tokens + self.output_tokens + self.cache_read_tokens + self.cache_creation_tokens;
    }

    /// 真实消耗 token (不含缓存读取，因为缓存读取不消耗新计算)
    /// 参考 cc-switch 的 real_total_tokens 概念
    pub fn real_total(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_creation_tokens + self.cache_read_tokens
    }

    /// 缓存命中率 = cache_read / (input + cache_creation + cache_read)
    /// 参考 cc-switch 的 cache_hit_rate 计算
    pub fn cache_hit_rate(&self) -> f64 {
        let cacheable_input = self.input_tokens + self.cache_creation_tokens + self.cache_read_tokens;
        if cacheable_input > 0 {
            self.cache_read_tokens as f64 / cacheable_input as f64
        } else {
            0.0
        }
    }
}

/// 日汇总统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySummary {
    pub date: String, // YYYY-MM-DD
    pub provider: String,
    pub model: String,
    pub collector: String,
    pub request_count: u64,
    pub total_input: u64,
    pub total_output: u64,
    pub total_cache_read: u64,
    pub total_cache_creation: u64,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub total_cost_cny: f64,
    pub avg_latency_ms: Option<u64>,
    pub cache_hit_rate: f64,
}

/// Provider 汇总统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStats {
    pub provider: String,
    pub request_count: u64,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub total_cost_cny: f64,
    pub cache_hit_rate: f64,
}

/// Model 汇总统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStats {
    pub provider: String,
    pub model: String,
    pub request_count: u64,
    pub total_input: u64,
    pub total_output: u64,
    pub total_cache_read: u64,
    pub total_cache_creation: u64,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub total_cost_cny: f64,
    pub cache_hit_rate: f64,
}

/// Tool (Collector) 汇总统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStats {
    pub collector: String,
    pub tool: Option<String>,
    pub request_count: u64,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub total_cost_cny: f64,
}

/// Project 汇总统计（从 source_file / session 路径推断）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStats {
    pub project: String,
    pub request_count: u64,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub total_cost_cny: f64,
}

/// 会话级汇总统计（借鉴 codex-token-hud 的 session grouping，按 session_id + provider + model 聚合）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    pub session_id: String,
    pub provider: String,
    pub model: String,
    pub collector: String,
    pub request_count: u64,
    pub total_input: u64,
    pub total_output: u64,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub total_cost_cny: f64,
    /// 会话首个请求时间 (ISO 8601)
    pub first_seen: String,
    /// 会话末个请求时间 (ISO 8601)
    pub last_seen: String,
    /// 会话时长 = last_seen - first_seen (秒)
    pub duration_secs: u64,
}

/// Agent tool invocation ranking (Bash, Read, MCP tools, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocationStats {
    pub name: String,
    pub invocation_count: u64,
}

/// Skill usage ranking (Claude Skill tool / explicit attribution)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInvocationStats {
    pub name: String,
    pub invocation_count: u64,
}

/// Single day in the token usage heatmap (calendar cell).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeatmapDay {
    pub date: String,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub total_cost_cny: f64,
    pub request_count: u64,
}

/// Token usage heatmap for a date range (default ~6 months).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenHeatmap {
    pub period_days: u32,
    pub start_date: String,
    pub end_date: String,
    pub days: Vec<HeatmapDay>,
}

/// Hour-of-week activity cell (weekday 0=Sunday..6=Saturday, hour 0..23).
///
/// Grouped in the server's local timezone on purpose: this view answers
/// "when during the day/week do I actually burn tokens", which is only
/// meaningful in the user's own rhythm (server and dashboard are the same
/// machine in the local-first deployment). Daily trends/heatmap stay UTC.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HourOfWeekCell {
    pub weekday: u8,
    pub hour: u8,
    pub total_tokens: u64,
    pub request_count: u64,
}

/// 总体汇总 (Dashboard 概览)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewStats {
    pub total_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_reasoning_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub total_cost_cny: f64,
    pub cache_hit_rate: f64,
    pub success_rate: f64,
}

/// 请求列表过滤器
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestFilter {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub collector: Option<String>,
    pub tool: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub min_tokens: Option<u64>,
    pub max_tokens: Option<u64>,
    pub search: Option<String>,
}

/// 分页请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    pub page: u32,
    pub page_size: u32,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            page: 0,
            page_size: 50,
        }
    }
}

/// 分页结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

/// Provider 枚举 (用于自动识别)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    OpenAI,
    Anthropic,
    DeepSeek,
    Qwen,
    Moonshot,
    MiniMax,
    GLM,
    Baichuan,
    StepFun,
    SiliconFlow,
    Volcengine,
    Yi,
    Zhipu,
    Groq,
    OpenRouter,
    XAI,
    Mistral,
    Google,
    CcSwitch,
    Unknown(String),
}

impl Provider {
    pub fn from_url_and_model(url: &str, model: &str) -> Self {
        match () {
            _ if url.contains("api.openai.com") => Self::OpenAI,
            _ if url.contains("api.anthropic.com") => Self::Anthropic,
            _ if url.contains("api.deepseek.com") => Self::DeepSeek,
            _ if url.contains("dashscope.aliyuncs.com") => Self::Qwen,
            _ if url.contains("api.moonshot.cn") => Self::Moonshot,
            _ if url.contains("api.minimax.chat") => Self::MiniMax,
            _ if url.contains("open.bigmodel.cn") => Self::GLM,
            _ if url.contains("api.baichuan-ai.com") => Self::Baichuan,
            _ if url.contains("api.stepfun.com") => Self::StepFun,
            _ if url.contains("api.siliconflow.cn") => Self::SiliconFlow,
            _ if url.contains("ark.cn-beijing.volces.com") => Self::Volcengine,
            _ if url.contains("api.lingyiwanwu.com") => Self::Yi,
            _ if url.contains("api.zhipuai.cn") => Self::Zhipu,
            _ if url.contains("api.groq.com") => Self::Groq,
            _ if url.contains("openrouter.ai") => Self::OpenRouter,
            _ if url.contains("api.x.ai") => Self::XAI,
            _ if url.contains("api.mistral.ai") => Self::Mistral,
            _ if url.contains("generativelanguage.googleapis.com") => Self::Google,
            // 按模型名 fallback
            _ if model.starts_with("deepseek") => Self::DeepSeek,
            _ if model.starts_with("qwen") => Self::Qwen,
            _ if model.starts_with("moonshot") => Self::Moonshot,
            _ if model.starts_with("glm") || model.starts_with("chatglm") => Self::GLM,
            _ if model.starts_with("doubao") => Self::Volcengine,
            _ if model.starts_with("yi-") => Self::Yi,
            _ if model.starts_with("baichuan") => Self::Baichuan,
            _ if model.starts_with("step-") => Self::StepFun,
            _ if model.starts_with("claude") => Self::Anthropic,
            _ if model.starts_with("gpt-") || model.starts_with("o1") || model.starts_with("o3") || model.starts_with("o4") => Self::OpenAI,
            _ if model.starts_with("gemini") => Self::Google,
            _ if model.starts_with("mistral") || model.starts_with("codestral") => Self::Mistral,
            _ => Self::Unknown(model.to_string()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::OpenAI => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::DeepSeek => "DeepSeek",
            Self::Qwen => "Qwen",
            Self::Moonshot => "Moonshot",
            Self::MiniMax => "MiniMax",
            Self::GLM => "GLM",
            Self::Baichuan => "Baichuan",
            Self::StepFun => "StepFun",
            Self::SiliconFlow => "SiliconFlow",
            Self::Volcengine => "Volcengine",
            Self::Yi => "Yi",
            Self::Zhipu => "Zhipu",
            Self::Groq => "Groq",
            Self::OpenRouter => "OpenRouter",
            Self::XAI => "xAI",
            Self::Mistral => "Mistral",
            Self::Google => "Google",
            Self::CcSwitch => "CcSwitch",
            Self::Unknown(s) => s,
        }
    }

    /// 从 OpenRouter 格式的模型名中提取真实 provider
    /// 例如 "anthropic/claude-sonnet-4-20250514" -> "anthropic"
    pub fn parse_openrouter_model(model: &str) -> (Option<String>, &str) {
        if let Some((provider_part, model_part)) = model.split_once('/') {
            (Some(provider_part.to_string()), model_part)
        } else {
            (None, model)
        }
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Monthly budget threshold (USD). Stored in app_config as JSON.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BudgetConfig {
    pub monthly_usd: Option<f64>,
    #[serde(default)]
    pub enabled: bool,
}

/// Desktop widget window state (Tauri 悬浮小组件). Stored in app_config as JSON.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WidgetConfig {
    /// Whether the widget window is shown.
    #[serde(default)]
    pub visible: bool,
    /// Last window position (physical pixels); None = system default placement.
    pub x: Option<i32>,
    pub y: Option<i32>,
}

/// User pricing overrides and exchange rate. Stored in app_config as JSON.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PricingConfig {
    pub usd_to_cny: Option<f64>,
    #[serde(default)]
    pub overrides: Vec<crate::pricing::PricingEntry>,
}

/// Per-collector enable flags. Missing keys default to enabled.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CollectorsConfig {
    #[serde(default)]
    pub enabled: std::collections::HashMap<String, bool>,
}

/// General application preferences.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeneralConfig {
    /// Background auto-scan interval in minutes. 0 = disabled.
    #[serde(default)]
    pub auto_scan_interval_minutes: u32,
    /// Launch the desktop app when the system starts (Tauri only).
    #[serde(default)]
    pub launch_at_startup: bool,
}

/// Data retention and archival preferences.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DataConfig {
    /// Delete records older than this many days. 0 = keep all records.
    #[serde(default)]
    pub retention_days: u32,
}

/// One subscription plan the user pays a flat monthly fee for.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubscriptionTier {
    /// Human-readable label, e.g. "Claude Max".
    pub label: String,
    /// Flat monthly fee in USD.
    pub monthly_usd: f64,
}

/// Subscription tiers used to compute "已省 X%" against API-equivalent cost.
/// Stored in app_config as JSON.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SubscriptionConfig {
    #[serde(default)]
    pub tiers: Vec<SubscriptionTier>,
    #[serde(default)]
    pub enabled: bool,
}

/// Rolling quota window kind (classified by `window_duration_mins`, not slot order).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexQuotaWindowKind {
    FiveHour,
    SevenDay,
    Other,
}

/// One Codex subscription rolling window from `account/rateLimits/read`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexQuotaWindow {
    pub kind: CodexQuotaWindowKind,
    /// Used percentage (0–100). `None` when unavailable — display `--`, not 0.
    pub used_percent: Option<i32>,
    /// Remaining percentage (0–100). Derived from `used_percent` when present.
    pub remaining_percent: Option<i32>,
    pub window_duration_mins: Option<i64>,
    /// Unix timestamp (seconds) when the window resets.
    pub resets_at: Option<i64>,
}

/// Cached Codex account quota snapshot (app-server JSON-RPC).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexQuotaSnapshot {
    pub fetched_at: DateTime<Utc>,
    pub source: String,
    pub plan_type: Option<String>,
    pub five_hour: Option<CodexQuotaWindow>,
    pub seven_day: Option<CodexQuotaWindow>,
    pub rate_limit_reached: bool,
}

/// API wrapper: cached snapshot plus optional fetch error when refresh fails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexQuotaResponse {
    pub snapshot: Option<CodexQuotaSnapshot>,
    pub error: Option<String>,
}

/// Cached Claude Code account quota from a local statusLine snapshot file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClaudeQuotaSnapshot {
    pub fetched_at: DateTime<Utc>,
    pub source: String,
    /// Path of the snapshot file that was read.
    pub snapshot_path: Option<String>,
    /// When the statusLine payload was captured (from snapshot metadata).
    pub captured_at: Option<DateTime<Utc>>,
    /// True when `captured_at` is older than 15 minutes (codexU stale threshold).
    pub is_stale: bool,
    pub five_hour: Option<CodexQuotaWindow>,
    pub seven_day: Option<CodexQuotaWindow>,
}

/// API wrapper for Claude quota cache / refresh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeQuotaResponse {
    pub snapshot: Option<ClaudeQuotaSnapshot>,
    pub error: Option<String>,
}
