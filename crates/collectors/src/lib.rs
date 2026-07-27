use alltokens_core::model::{ClaudeQuotaSnapshot, CodexQuotaSnapshot, UsageRecord};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::PathBuf;

/// Optional live quota snapshot from a tool's app-server or status API.
#[async_trait]
pub trait QuotaProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn fetch_quota(&self) -> Result<Option<CodexQuotaSnapshot>>;
}

/// Claude Code active quota from local statusLine snapshot cache.
#[async_trait]
pub trait ClaudeQuotaProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn fetch_claude_quota(&self) -> Result<Option<ClaudeQuotaSnapshot>>;
}

pub mod cc_switch;
pub mod claude_code;
pub mod claude_quota;
pub mod cline;
pub mod codebuddy;
pub mod codex;
pub mod codex_quota;
pub mod copilot;
pub mod cursor;
pub mod generic;
pub mod hermes;
pub mod mcp;
pub mod opencode;
pub mod openclaw;
pub mod paths;
pub mod probe;
pub mod trae;
pub mod windsurf;
pub mod zcode;
pub mod zed;

/// 所有数据采集器的统一接口
#[async_trait]
pub trait Collector: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>>;
    fn watch_paths(&self) -> Vec<PathBuf>;
}

/// 注册所有内置采集器
pub fn register_collectors() -> Vec<Box<dyn Collector>> {
    let mut c: Vec<Box<dyn Collector>> = Vec::new();

    macro_rules! reg {
        ($expr:expr) => {{
            let collector = $expr;
            if collector.is_available() { tracing::info!("Detected: {}", collector.name()); }
            c.push(Box::new(collector));
        }};
    }

    // CLI
    reg!(claude_code::ClaudeCodeCollector::new());
    reg!(codex::CodexCollector::new());
    reg!(opencode::OpenCodeCollector::new());
    reg!(openclaw::OpenClawCollector::new());
    reg!(hermes::HermesCollector::new());
    reg!(generic::GrokBuildCollector::new());
    reg!(generic::KimiCollector::new());
    reg!(generic::QwenCollector::new());
    reg!(generic::PiCollector::new());
    reg!(generic::MiMoCodeCollector::new());

    // AI IDE
    reg!(cursor::CursorCollector::new());
    reg!(windsurf::WindsurfCollector::new());
    reg!(zed::ZedCollector::new());
    reg!(trae::TraeCollector::new());
    reg!(generic::QoderCollector::new());
    reg!(zcode::ZCodeCollector::new());
    reg!(generic::AntigravityCollector::new());

    // IDE / VS Code 扩展
    reg!(copilot::CopilotCollector::new());
    reg!(cline::ClineCollector::new_cline());
    reg!(cline::ClineCollector::new_roo_code());
    reg!(cline::ClineCollector::new_kilo_code());
    reg!(codebuddy::CodeBuddyCollector::new());

    // 第三方导入
    reg!(cc_switch::CcSwitchCollector::new());

    if paths::is_wsl() {
        tracing::info!("WSL environment detected - scanning Windows paths");
    }

    c
}
