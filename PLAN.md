# AllTokens — 全栈 API Token 用量追踪器

> 追踪一切 AI API 调用的 token 用量与成本。不管是 CLI、IDE、Agent 还是插件。

## 定位

个人工具，开源可分享。轻量、本地优先、隐私安全。

## 技术栈

| 层 | 选型 | 理由 |
|---|---|---|
| 核心引擎 | Rust | 性能、单二进制、跨平台 |
| 桌面端 | Tauri 2.x | 轻量 WebView，Rust 原生后端 |
| Web UI | React 19 + Tailwind 4 + 纯 CSS/SVG 图表 | 轻量、无额外图表库依赖 |
| 存储 | SQLite (rusqlite) | 零配置、单文件、够用 |
| Token 计数 | tiktoken-rs (OpenAI) + 自研 tokenizer | 覆盖主流模型 |
| 代理层 | hyper + tokio-rustls | 高性能 async HTTP(S) 代理 |

## 架构总览

```
数据采集 (三层，优先级递减)
┌──────────────────────────────────────────────┐
│  Layer 1: 透明代理 (Phase 3)                  │
│  · MITM HTTPS，拦截已知 API endpoint          │
│  · 旁路解析 usage，零侵入                      │
│                                              │
│  Layer 2: 日志/文件解析 (Phase 1, MVP 核心)    │
│  · 读取各工具的本地日志/DB/JSON                │
│  · 定时轮询 + fswatch 文件变动                 │
│                                              │
│  Layer 3: MCP Server / Webhook (Phase 4 ✅)     │
│  · 工具主动推送 usage 数据                     │
└──────────────┬───────────────────────────────┘
               │
               ▼
┌──────────────────────────────────────────────┐
│           Core Engine                         │
│  · 统一数据模型 (UsageRecord)                 │
│  · 成本计算 (可配置 pricing table)            │
│  · 缓存命中率计算                             │
│  · 自动聚合 (小时/天/月 summary)              │
└──────────────┬───────────────────────────────┘
               │
               ▼
┌──────────────────────────────────────────────┐
│           Storage (SQLite)                    │
│  · api_requests 表 (请求级)                   │
│  · daily_summary 表 (自动聚合)                │
│  · pricing 表 (可覆盖)                        │
│  · tools 表 (工具注册)                        │
└──────────────┬───────────────────────────────┘
               │
    ┌──────────┼──────────┐
    ▼          ▼          ▼
┌────────┐ ┌────────┐ ┌────────┐
│  CLI   │ │  Web   │ │ Tauri  │
│ 终端查询│ │Dashboard│ │桌面应用 │
└────────┘ └────────┘ └────────┘
```

## 目录结构

```
alltokens/
├── Cargo.toml                   # workspace root
├── crates/
│   ├── core/                    # 核心库：数据模型、存储、定价
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── model.rs         # UsageRecord, DailySummary 等
│   │       ├── storage.rs       # SQLite 操作
│   │       ├── schema.sql       # DDL
│   │       ├── pricing.rs       # 成本计算 + 定价表
│   │       └── aggregator.rs    # 自动聚合逻辑
│   │
│   ├── collectors/              # 数据采集器
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs           # Collector trait + 23 个采集器注册
│   │       ├── claude_code.rs   # Claude Code 日志解析
│   │       ├── cursor.rs        # Cursor 日志解析
│   │       ├── openclaw.rs      # OpenClaw 日志解析
│   │       ├── generic.rs       # 通用 JSON/JSONL 采集器 (Kimi/Qoder/Trae 等)
│   │       ├── trae.rs          # TraeCollector re-export，实际实现在 generic.rs
│   │       ├── codex.rs         # Codex 日志解析
│   │       ├── cline.rs         # Cline (VS Code) 日志解析
│   │       ├── codebuddy.rs     # CodeBuddy 日志解析
│   │       ├── copilot.rs       # GitHub Copilot 日志解析
│   │       ├── hermes.rs        # Hermes 日志解析
│   │       ├── opencode.rs      # OpenCode 日志解析
│   │       ├── cc_switch.rs     # cc-switch SQLite 导入
│   │       ├── windsurf.rs      # Windsurf 日志解析
│   │       ├── zcode.rs         # ZCode 日志解析
│   │       ├── zed.rs           # Zed 日志解析
│   │       ├── paths.rs         # WSL / 跨平台路径扫描
│   │       └── mcp.rs           # MCP Server (Phase 4，stdio JSON-RPC 查询 + 推送)
│   │
│   ├── proxy/                   # 透明代理引擎 (Phase 3)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── server.rs        # HTTP(S) 代理服务器
│   │       ├── intercept.rs     # 响应解析 & usage 提取
│   │       ├── mitm.rs          # MITM TLS（双向 TLS + chunked/SSE 解码 + 压缩解压）
│   │       ├── ca.rs            # 自签 CA + 动态叶子证书签发
│   │       └── ca_install.rs    # CA 一键装入系统信任库（三平台）
│   │
│   ├── web/                     # Web API 服务
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── scan.rs          # 扫描调度
│   │       ├── ws.rs            # WebSocket 实时推送
│   │       └── events.rs        # 事件总线
│   │
│   └── cli/                     # CLI 工具
│       ├── Cargo.toml
│       └── src/
│           └── main.rs          # clap CLI
│
├── frontend/                    # React 前端
│   ├── package.json
│   ├── vite.config.ts
│   ├── tailwind.config.ts
│   ├── index.html
│   └── src/
│       ├── main.tsx
│       ├── App.tsx
│       ├── api/                 # API 调用封装
│       ├── components/          # Dashboard 组件
│       │   ├── StatsCards.tsx
│       │   ├── TrendPanel.tsx       # 趋势面板（metric + 粒度切换）
│       │   ├── StatusBar.tsx        # 底部状态栏
│       │   ├── TokenHeatmap.tsx
│       │   ├── HourOfWeekHeatmap.tsx  # 星期×小时活动热力图
│       │   ├── ProviderPie.tsx
│       │   ├── ModelBreakdown.tsx
│       │   ├── ToolBreakdown.tsx
│       │   ├── ProjectBreakdown.tsx
│       │   ├── ToolInvocationBreakdown.tsx
│       │   ├── SkillInvocationBreakdown.tsx
│       │   ├── CodexQuotaCard.tsx
│       │   ├── ClaudeQuotaCard.tsx
│       │   ├── BudgetAlert.tsx
│       │   ├── RequestFilters.tsx
│       │   ├── RequestTable.tsx
│       │   ├── RequestDetailModal.tsx
│       │   ├── Layout.tsx
│       │   └── ui/                # 共享控件 (SegmentedControl, FreshnessLabel)
│       ├── pages/
│       │   ├── Dashboard.tsx
│       │   └── Settings.tsx
│       └── hooks/
│           ├── useWebSocket.ts
│           ├── useStats.ts
│           ├── useBudget.ts
│           ├── useCodexQuota.ts
│           ├── useClaudeQuota.ts
│           └── useTheme.ts
│
├── src-tauri/                   # Tauri 桌面端
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   ├── src/
│   │   ├── main.rs
│   │   └── lib.rs               # Tauri commands
│   └── icons/
│
├── pricing/                     # 定价数据
│   ├── builtin.toml             # 内置定价表
│   └── README.md                # 定价维护说明
│
├── scripts/
│   └── dev-pipeline.ps1         # 开发流水线脚本（CA 安装已内建为 `alltokens ca install`，无需脚本）
│
└── docs/                        # 预留目录（暂为空；文档见 README.md / STATUS.md / pricing/README.md）
```

## 数据模型

### api_requests (请求级记录)

```sql
CREATE TABLE IF NOT EXISTS api_requests (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    -- 时间
    timestamp         TEXT NOT NULL,              -- ISO 8601
    -- 来源
    collector         TEXT NOT NULL,              -- 'claude_code' | 'cursor' | 'proxy' | ...
    tool              TEXT,                       -- 用户可标注的工具名
    provider          TEXT,                       -- 'openai' | 'anthropic' | 'deepseek' | 'qwen' | ...
    model             TEXT,                       -- 'gpt-4o' | 'claude-sonnet-4-20250514' | ...
    -- Token 用量
    input_tokens      INTEGER NOT NULL DEFAULT 0,
    output_tokens     INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0, -- 缓存命中 (prompt caching)
    cache_creation_tokens INTEGER NOT NULL DEFAULT 0, -- 缓存创建
    total_tokens      INTEGER NOT NULL DEFAULT 0, -- 冗余但查询方便
    -- 成本
    cost_usd          REAL NOT NULL DEFAULT 0.0,
    cost_cny          REAL NOT NULL DEFAULT 0.0,  -- 人民币成本 (按汇率换算)
    -- 元数据
    latency_ms        INTEGER,
    is_stream         BOOLEAN NOT NULL DEFAULT FALSE,
    status_code       INTEGER,
    session_id        TEXT,                       -- 会话/项目标识
    request_id        TEXT,                       -- API 返回的 request id
    -- 上下文
    source_file       TEXT,                       -- 日志来源文件路径
    raw_json          TEXT,                       -- 原始 JSON (可选保留, 调试用)
    notes             TEXT                        -- 用户备注
);

CREATE INDEX IF NOT EXISTS idx_requests_timestamp ON api_requests(timestamp);
CREATE INDEX IF NOT EXISTS idx_requests_provider ON api_requests(provider);
CREATE INDEX IF NOT EXISTS idx_requests_model ON api_requests(model);
CREATE INDEX IF NOT EXISTS idx_requests_collector ON api_requests(collector);
```

### daily_summary (自动聚合)

```sql
CREATE TABLE IF NOT EXISTS daily_summary (
    date              TEXT NOT NULL,              -- YYYY-MM-DD
    provider          TEXT NOT NULL,
    model             TEXT NOT NULL,
    collector         TEXT NOT NULL,
    request_count     INTEGER NOT NULL DEFAULT 0,
    total_input       INTEGER NOT NULL DEFAULT 0,
    total_output      INTEGER NOT NULL DEFAULT 0,
    total_cache_read  INTEGER NOT NULL DEFAULT 0,
    total_cache_creation INTEGER NOT NULL DEFAULT 0,
    total_tokens      INTEGER NOT NULL DEFAULT 0,
    total_cost_usd    REAL NOT NULL DEFAULT 0.0,
    total_cost_cny    REAL NOT NULL DEFAULT 0.0,
    avg_latency_ms    INTEGER,
    PRIMARY KEY (date, provider, model, collector)
);
```

### pricing (可配置定价)

```sql
CREATE TABLE IF NOT EXISTS pricing (
    provider          TEXT NOT NULL,
    model             TEXT NOT NULL,
    input_per_mtok    REAL NOT NULL,              -- 每百万 input token (USD)
    output_per_mtok   REAL NOT NULL,              -- 每百万 output token (USD)
    cache_read_per_mtok REAL DEFAULT 0.0,         -- 缓存读取
    cache_create_per_mtok REAL DEFAULT 0.0,       -- 缓存创建
    effective_from    TEXT,                       -- 生效日期 (NULL = 永久)
    source            TEXT DEFAULT 'builtin',     -- 'builtin' | 'user'
    PRIMARY KEY (provider, model, effective_from)
);
```

## 支持的工具 & 数据源

### Phase 1 MVP 日志解析

| 工具 | 数据源 | 路径 | 说明 |
|---|---|---|---|
| Claude Code | JSON usage files | `~/.claude/projects/*/` | 每个请求有完整 usage |
| Cursor | tokscale cache | `~/.config/tokscale/cursor-cache/` | token-monitor 格式 |
| Codex | session files | `~/.codex/sessions/` | OpenAI Codex CLI |
| OpenClaw | agent logs | `~/.openclaw/agents/*/logs/` | OpenClaw agent 日志 |
| Cline | VS Code storage | VS Code globalStorage | 任务级 token 统计 |
| Roo Code | VS Code storage | VS Code globalStorage | 类似 Cline |
| Kilo Code | VS Code storage | VS Code globalStorage | 类似 Cline |
| Windsurf | local DB | `~/.local/share/windsurf/` | SQLite DB |
| Zed | thread DB | `~/.local/share/zed/threads/threads.db` | SQLite |

### Phase 1.5 补充 (需要逆向/社区贡献)

| 工具 | 数据源 | 状态 |
|---|---|---|
| Qoder | 待确认本地日志格式 | 需要安装后分析 |
| Trae | 待确认本地日志格式 | ByteDance AI IDE |
| Kimi CLI | `~/.kimi/sessions/` | 待确认 |
| Qwen CLI | 待确认 | 通义千问 CLI |
| MiMo Code | `~/.local/share/mimocode/` | 待确认 |
| ZCode/GLM | `~/.zcode/projects/` | 待确认 |
| GitHub Copilot | `~/.copilot/otel/` | OpenTelemetry 格式 |
| WorkBuddy | `~/.workbuddy/workbuddy.db` | SQLite |
| Kiro | `~/.kiro/sessions/cli/` | 待确认 |

### 国内模型 Provider 识别

大多数国内模型使用 OpenAI 兼容格式，通过 `base_url` 识别 provider：

```rust
fn identify_provider(base_url: &str, model: &str) -> Provider {
    match () {
        _ if base_url.contains("api.deepseek.com") => Provider::DeepSeek,
        _ if base_url.contains("dashscope.aliyuncs.com") => Provider::Qwen,
        _ if base_url.contains("api.moonshot.cn") => Provider::Moonshot,
        _ if base_url.contains("api.minimax.chat") => Provider::MiniMax,
        _ if base_url.contains("open.bigmodel.cn") => Provider::GLM,
        _ if base_url.contains("api.baichuan-ai.com") => Provider::Baichuan,
        _ if base_url.contains("api.stepfun.com") => Provider::StepFun,
        _ if base_url.contains("api.siliconflow.cn") => Provider::SiliconFlow,
        _ if base_url.contains("ark.cn-beijing.volces.com") => Provider::Volcengine,
        _ if base_url.contains("api.lingyiwanwu.com") => Provider::Yi,
        _ if base_url.contains("api.zhipuai.cn") => Provider::Zhipu,
        _ if model.starts_with("deepseek") => Provider::DeepSeek,
        _ if model.starts_with("qwen") => Provider::Qwen,
        _ if model.starts_with("moonshot") => Provider::Moonshot,
        _ if model.starts_with("glm") => Provider::GLM,
        _ => Provider::Unknown,
    }
}
```

## 内置定价表 (pricing/builtin.toml)

```toml
# 格式: [provider.model]
# input/output: 每百万 token 价格 (USD)
# cache_read/cache_create: 缓存相关价格

# OpenAI
[openai."gpt-4o"]
input = 2.50
output = 10.00
cache_read = 1.25

[openai."gpt-4o-mini"]
input = 0.15
output = 0.60
cache_read = 0.075

[openai."o3"]
input = 2.00
output = 8.00
cache_read = 0.50

[openai."o4-mini"]
input = 1.10
output = 4.40
cache_read = 0.275

# Anthropic
[anthropic."claude-sonnet-4-20250514"]
input = 3.00
output = 15.00
cache_read = 0.30
cache_create = 3.75

[anthropic."claude-haiku-3.5"]
input = 0.80
output = 4.00
cache_read = 0.08
cache_create = 1.00

# DeepSeek
[deepseek."deepseek-chat"]
input = 0.27
output = 1.10
cache_read = 0.07

[deepseek."deepseek-reasoner"]
input = 0.55
output = 2.19
cache_read = 0.14

# Qwen (通义千问)
[qwen."qwen-plus"]
input = 0.80
output = 2.00
cache_read = 0.20

[qwen."qwen-turbo"]
input = 0.05
output = 0.20

[qwen."qwen-max"]
input = 2.40
output = 9.60

# Moonshot (Kimi)
[moonshot."moonshot-v1-8k"]
input = 1.20
output = 1.20

[moonshot."moonshot-v1-32k"]
input = 2.40
output = 2.40

[moonshot."moonshot-v1-128k"]
input = 6.00
output = 6.00

# MiniMax
[minimax."MiniMax-Text-01"]
input = 1.00
output = 8.00

# GLM (智谱)
[glm."glm-4-plus"]
input = 5.00
output = 5.00

[glm."glm-4-flash"]
input = 0.00  # 免费
output = 0.00

# SiliconFlow (硅基流动)
[siliconflow."deepseek-ai/DeepSeek-V3"]
input = 0.27
output = 1.10

[siliconflow."Qwen/Qwen2.5-72B-Instruct"]
input = 0.35
output = 1.35

# Volcengine (火山引擎 / 豆包)
[volcengine."doubao-1.5-pro-32k"]
input = 0.80
output = 2.00

[volcengine."doubao-1.5-lite-32k"]
input = 0.30
output = 0.60

# StepFun (阶跃星辰)
[stepfun."step-1-8k"]
input = 2.00
output = 2.00

# Baichuan (百川)
[baichuan."Baichuan4"]
input = 3.00
output = 9.00

# Yi (零一万物)
[yi."yi-large"]
input = 2.40
output = 2.40

# Groq (托管开源模型)
[groq."llama-3.3-70b"]
input = 0.59
output = 0.79

# OpenRouter (聚合)
# OpenRouter 的模型名带 provider 前缀，如 "anthropic/claude-sonnet-4-20250514"
# 在解析时需要剥离前缀再匹配
```

## Collector Trait

```rust
/// 所有数据采集器的统一接口
#[async_trait]
pub trait Collector: Send + Sync {
    /// 采集器唯一标识
    fn id(&self) -> &str;
    
    /// 采集器显示名称
    fn name(&self) -> &str;
    
    /// 检测该工具是否已安装 / 是否有可解析数据
    fn is_available(&self) -> bool;
    
    /// 收集新的 usage 记录
    /// `since`: 只返回此时间之后的记录 (增量采集)
    async fn collect(&self, since: Option<DateTime<Utc>>) -> Result<Vec<UsageRecord>>;
    
    /// 返回数据源路径 (用于 fswatch 监听)
    fn watch_paths(&self) -> Vec<PathBuf>;
}
```

## CLI 设计

```bash
# 初始化 (创建 DB, 安装 CA 证书如果需要)
alltokens init

# 扫描所有可用工具并采集一次
alltokens scan

# 后台自动扫描
alltokens daemon [--interval 15] [--once]  # 常驻：定时扫描 + 按保留天数自动归档
alltokens serve                          # Web 服务运行期间按 Settings 间隔自动扫描

# MCP Server (Phase 4)
alltokens mcp                            # stdio JSON-RPC：查询统计 + report_usage 推送

# 查询
alltokens today                          # 今日汇总
alltokens list                           # 最近 20 条请求
alltokens list --provider deepseek       # 按 provider 过滤
alltokens list --model gpt-4o --last 7d  # 模型 + 时间过滤
alltokens list --tool cursor             # 按工具过滤

# 统计
alltokens stats                          # 总体统计
alltokens stats --by provider            # 按 provider 分组
alltokens stats --by model               # 按模型分组
alltokens stats --by tool                # 按工具分组
alltokens stats --by day --last 30d      # 最近 30 天趋势

# 成本
alltokens cost                           # 总成本
alltokens cost --currency cny            # 人民币显示
alltokens budget set --monthly 100       # 设置月预算
alltokens budget status                  # 预算使用情况

# 定价管理
alltokens pricing list                   # 查看所有定价
# CLI 目前只有 pricing list；自定义定价通过 Web API / Settings UI 写入覆盖配置

# Web UI
alltokens serve                          # 启动 Web 服务 (默认 :3210，含后台自动扫描)
alltokens serve --port 8080              # 自定义端口

# 代理 (Phase 3)
alltokens proxy start [--listen 127.0.0.1:7890]  # 启动转发代理
alltokens proxy status

# 采集器探测 (codexU)
alltokens probe [--json]                          # 列出全部采集器检测状态
alltokens probe codex [--json]                    # Codex 数据源 + 额度快照
alltokens probe claude [--json]                   # Claude 数据源 + 额度快照
alltokens probe cursor|opencode|windsurf [--json] # 其他采集器探测
```

## Tauri 桌面端

### 功能

- 系统托盘常驻（tooltip/title 显示 Codex/Claude 5h/7d 额度，2 分钟缓存刷新）
- 主窗口: 内嵌 Web Dashboard（`serve` 同源 API）
- 关闭到托盘、预算 OS 通知（80%/100%）
- 开机自启（`tauri-plugin-autostart`）
- 后台自动扫描 + `POST /api/events/scan-complete` 推送 WebSocket

### Tauri Commands (Rust → JS 桥接)

```rust
#[tauri::command]
fn get_overview(last: Option<String>) -> Result<OverviewStats, String>;

#[tauri::command]
fn get_providers(last: Option<String>) -> Result<Vec<ProviderStats>, String>;

#[tauri::command]
fn get_models(last: Option<String>) -> Result<Vec<ModelStats>, String>;

#[tauri::command]
fn get_trends(last: Option<String>) -> Result<Vec<DailySummary>, String>;

#[tauri::command]
fn get_requests(last: Option<String>, page: Option<u32>, page_size: Option<u32>)
    -> Result<PaginatedResult<UsageRecord>, String>;

#[tauri::command]
async fn run_scan(app: tauri::AppHandle) -> Result<ScanResult, String>;
```

> 其余 Dashboard 功能通过内嵌 Web API 访问；额度实时刷新使用 `GET /api/quota/*?refresh=true`。

## cc-switch 参考分析

cc-switch 本身就是一个 API 中继/代理，天然拦截所有请求。其使用记录模块 (`usage_stats.rs`, 150KB) 值得参考：

### 数据模型借鉴

cc-switch 的 `UsageSummary` 包含：
- `total_requests`, `total_cost`
- `total_input_tokens`, `total_output_tokens`
- `total_cache_creation_tokens`, `total_cache_read_tokens`
- `real_total_tokens` = input + output + cache_creation + cache_read
- `cache_hit_rate` = cache_read / (input + cache_creation + cache_read)
- `success_rate`

其 `model_pricing` 表：
- `input_cost_per_million`, `output_cost_per_million`
- `cache_read_cost_per_million`, `cache_creation_cost_per_million`

### AllTokens 与 cc-switch 的关系

cc-switch 用户如果同时用 AllTokens，AllTokens 可以从 cc-switch 的 SQLite DB (`usage_stats` 表) 直接导入数据，作为额外 Collector。

这覆盖了：Claude Code / Codex / Cursor / Gemini CLI / OpenCode / OpenClaw / Hermes 等通过 cc-switch 中继的工具。

### 聚合查询借鉴

cc-switch 提供的查询维度，AllTokens 也要支持：
- 按时间范围 (start_date, end_date)
- 按工具 (app_type)
- 按 provider (provider_name)
- 按模型 (model)
- 日趋势 (daily trends)
- 请求级日志 (带分页、过滤)
- 请求详情 (request detail)

## 开发阶段

### Phase 1: Core + CLI + 日志解析 MVP ✅

- [x] `crates/core`: 数据模型 + SQLite 存储 + 定价计算
- [x] `crates/collectors`: 23 个采集器 (含 WSL 支持)
- [x] `crates/cli`: CLI 工具 (init, scan, today, list, stats, cost, pricing, export, budget, serve, proxy, probe)
- [x] `crates/web`: axum Web API + WebSocket（20+ 端点）
- [x] `pricing/builtin.toml`: 内置定价表 (55 模型, 16 Provider)
- [x] 测试: 150 个（workspace 147 + Tauri 3），全部通过

### Phase 1.5: 更多 Collector + 国内模型 + WSL ✅

- [x] Claude Code, Cursor, Codex, OpenClaw, Hermes, OpenCode
- [x] Cline, Roo Code, Kilo Code, CodeBuddy (VS Code 扩展)
- [x] Windsurf, Zed, ZCode, Trae, Qoder, Grok Build (IDE)
- [x] Kimi CLI, Qwen CLI, MiMo Code, Antigravity, Pi (国产 CLI)
- [x] GitHub Copilot (代码助手)
- [x] cc-switch DB 导入 (第三方)
- [x] 国内模型定价表完善
- [x] `paths.rs` 模块: WSL 路径自动扫描
- [x] `README.md` 文档

### Phase 2: Web Dashboard + Tauri 桌面端 ✅

- [x] `frontend/`: React 仪表盘 (Vite + Tailwind 4，纯 CSS/SVG 图表)
- [x] 嵌入 Tauri（`src-tauri` + `frontend/dist`）
- [x] Tauri 应用: 系统托盘 + 关闭到托盘 + 预算 OS 通知 + 开机自启 + 托盘额度展示
- [x] WebSocket 实时推送（扫描完成事件）
- [x] Settings 全功能（定价/Collector/通用/预算/数据保留）
- [x] 数据导出 CSV/JSON/PDF（CLI + API + UI；PDF 为打印就绪 HTML 报表）
- [x] 预算告警（Dashboard + Tauri 通知）
- [x] codexU 分析维度：项目排行、Tool/Skill TOP、半年热力图、Codex/Claude 额度卡片

### Phase 3: 透明代理 (完成度 ~95%)

- [x] `crates/proxy`: HTTP 转发代理 + CONNECT 隧道
- [x] OpenAI/Anthropic/Qwen/Gemini 等 15 个 Provider host 响应格式解析 + SSE usage 提取
- [x] CLI `proxy start|status`；代理记录写入 SQLite
- [x] MITM TLS 处理 (`mitm.rs`) + chunked/SSE 流式解码 + gzip/deflate/br 解压
- [x] CA 证书管理 (`ca.rs` 自签 CA + 动态叶子证书；`ca_install.rs` 三平台一键安装)
- [x] 15 个 Provider host 拦截（含 Google Gemini `usageMetadata` 格式）

### Phase 4: 主动推送层 (Layer 3 ✅，2026-07-18)

- [x] Webhook: `POST /api/ingest`（单条/批量 ≤1000，定价自动计算，collector=`webhook`，WS 自动刷新）
- [x] MCP Server: `alltokens mcp`（stdio JSON-RPC，零新依赖；5 工具：get_overview / get_stats / list_requests / get_budget_status / report_usage）
- [x] 多设备同步（文件合并）: `sync export` / `sync import` + `Storage::merge_from` 幂等去重
- [x] 桌面小组件（Tauri 悬浮窗，2026-07-18，实施记录见 STATUS.md Phase 4）

### codexU 借鉴进度 (2026-07-14)

参考 [codexU](https://github.com/shanggqm/codexU)（macOS 菜单栏 Widget，专注 Codex + Claude Code）。

| 优先级 | 项 | 状态 |
|---|---|---|
| P0 | Codex JSONL delta + SQLite 回退 + `source_quality` + probe | ✅ |
| P0 | Codex app-server 额度 + `GET /api/quota/codex` + `CodexQuotaCard` | ✅ |
| P1 | Claude statusLine 额度 + `GET /api/quota/claude` + `ClaudeQuotaCard` + probe | ✅ |
| P1 | 项目排行 (`GET /api/projects`) + `ProjectBreakdown` | ✅ |
| P1 | Tool/Skill TOP (`/api/tools/ranking`, `/api/skills/ranking`) + Dashboard 图表 | ✅ |
| P1 | `alltokens probe` 列表 + cursor/opencode/windsurf probe | ✅ |
| P2 | 半年热力图 (`GET /api/heatmap`) + `TokenHeatmap` | ✅ |
| P2 | 托盘额度 tooltip/title 刷新（2 分钟缓存读 + 扫描后同步） | ✅ |
| P1/P2 | 今日任务看板 | ✅ |
| P2 | 订阅价值估算（羊毛进度） | ✅ |

## 构建 & 分发

```bash
# 开发
cargo build                          # 编译所有 crates
cargo run -p alltokens-cli -- scan   # 运行 CLI
cargo run -p alltokens-web           # 运行 Web 服务

# Tauri 开发
cd src-tauri && npx @tauri-apps/cli dev

# 测试
cargo test --workspace               # 166 个测试
cd src-tauri && cargo test           # 3 个测试（合计 169）

# Release
cargo build --release                # CLI + Web
cd src-tauri && npx @tauri-apps/cli build  # 桌面应用

# 分发产物
# - alltokens-cli (独立 CLI 二进制)
# - alltokens (Tauri 桌面应用, .dmg / .msi / .AppImage)
```

## 设计原则

1. **本地优先**: 所有数据存本地 SQLite，不联网，不上传
2. **零配置启动**: `alltokens scan` 一条命令就能用
3. **增量采集**: 记录上次采集位置，避免重复
4. **不侵入**: 不修改任何工具的行为，只读数据
5. **可扩展**: 新增 Collector 只需实现一个 trait
6. **定价灵活**: 内置 + 用户自定义，支持历史定价切换
