# AllTokens

> 追踪一切 AI API 调用的 token 用量与成本。不管是 CLI、IDE、Agent 还是插件。

本地优先、零配置、隐私安全。支持 CLI、Web Dashboard 和 Tauri 桌面端。

## 快速开始

```bash
# 编译 CLI
cargo build --release -p alltokens-cli

# 初始化 + 采集
./target/release/alltokens init
./target/release/alltokens scan

# 查看用量
./target/release/alltokens today
./target/release/alltokens stats --by provider
./target/release/alltokens list --last 7d

# 启动 Dashboard (Web UI + API)
./target/release/alltokens serve --port 3210
# 浏览器打开 http://127.0.0.1:3210
```

Windows 下将 `./target/release/alltokens` 替换为 `.\target\release\alltokens.exe`。

## 支持的工具 (23 个 Collector)

| 分类 | 工具 | 数据源 |
|---|---|---|
| **AI Agent** | Claude Code, Codex CLI, OpenClaw, Hermes, OpenCode | JSONL / JSON / SQLite |
| **IDE** | Cursor, Windsurf, Zed, ZCode, Trae, Qoder, Grok Build | JSON / SQLite |
| **VS Code 扩展** | Cline, Roo Code, Kilo Code, CodeBuddy | globalStorage |
| **国产 CLI** | Kimi CLI, Qwen CLI, Antigravity, Pi, MiMo Code | JSON / JSONL |
| **代码助手** | GitHub Copilot | JSON |
| **第三方导入** | cc-switch | SQLite 导入 |

所有 Collector **自动支持 WSL** — 扫描 `/mnt/c/Users/` 等 Windows 路径。可在 Settings 中单独启用/禁用。

## 支持的 Provider (16)

OpenAI · Anthropic · DeepSeek · Qwen · Moonshot · MiniMax · GLM · Volcengine · SiliconFlow · StepFun · Baichuan · Yi · Groq · Google · xAI · Mistral

内置 55 个模型定价、16 个 Provider（`pricing/builtin.toml`），支持用户自定义覆盖。

## CLI 命令

```bash
alltokens init                          # 初始化数据库
alltokens scan                          # 扫描采集数据
alltokens today                         # 今日汇总
alltokens list [--provider X] [--last 7d]  # 请求列表
alltokens stats --by provider|model|tool|day  # 统计
alltokens cost [--currency cny]         # 成本
alltokens pricing list                  # 查看定价表
alltokens export [--format csv|json|pdf] [-o file]  # 导出数据（pdf 为打印就绪 HTML 报表）
alltokens budget set --monthly 100      # 设置月预算 (USD)
alltokens budget status                 # 预算使用情况
alltokens serve [--port 3210]           # 启动 Web UI + API（含后台自动扫描）
alltokens daemon [--interval 15] [--once]  # 后台常驻：定时扫描 + 按保留天数自动归档
alltokens mcp                           # MCP Server (stdio)：AI 工具查询统计 / 推送 usage
alltokens sync export -o snapshot.db    # 导出一致快照（供其他设备合并）
alltokens sync import snapshot.db       # 合并其他设备的数据库（自动去重、幂等）
alltokens proxy start [--mitm] [--listen 127.0.0.1:7890]  # 启动代理（--mitm 解密 HTTPS）
alltokens proxy status                  # 代理状态说明
alltokens ca install|uninstall|status|path [--ca-dir DIR]  # CA 证书管理（系统信任库）
alltokens probe [--json]                 # 列出全部采集器检测状态
alltokens probe codex [--json]           # Codex 数据源探测 + 额度快照
alltokens probe claude [--json]          # Claude Code 数据源探测 + 额度快照
alltokens probe cursor|opencode|windsurf [--json]  # 其他采集器探测
```

`serve` / `daemon` 运行期间的日志同时写入 `~/.alltokens/logs/alltokens.log`（文件超过 5MB 时下次启动截断）。

## Web Dashboard

单页 Dashboard + Settings，功能包括：

- 统计卡片、`TrendPanel` 趋势面板（Tokens / Cost / Cache hit 指标切换 + Daily / Weekly 粒度切换）、`TokenHeatmap` 半年热力图 + `HourOfWeekHeatmap` 星期×小时活动热力图、Provider/Model/Tool/项目排行（分布面板默认 Top 8，支持展开/收起）
- `TodaySummary` 今日/本周/本月成本汇总 + 订阅制「羊毛价值」回本进度
- `SessionBreakdown` 会话级用量（tokens / 成本 / 时长 / 最近活跃）
- `ToolInvocationBreakdown` / `SkillInvocationBreakdown` Agent 工具与 Skill 调用 TOP
- `CodexQuotaCard` / `ClaudeQuotaCard` 额度卡片（5h / 7d 窗口，缺失显示 `--`；按剩余量状态着色 ≥50% 绿 / ≥20% 黄 / <20% 红，显示重置时间，Codex 限流时显示 rate-limited 徽章）
- 请求列表 + 详情弹窗（含 raw JSON + 每请求 Context-window 占用进度条）
- 按 provider/model/collector/tool 过滤（已激活条件显示为可单独清除的 chips），时间范围切换（周期选择器旁显示具体日期范围，如「7月12日 – 7月18日」，跨年补年份）
- Dashboard 内一键 Scan（触发全部 Collector 采集，完成后自动刷新）+ 头部数据新鲜度指示
- 底部 `StatusBar` 状态栏（周期汇总、WebSocket 连接状态、数据更新时间）
- 「Warm Ledger」设计系统：暖色系低饱和，浅色默认主题（深色主题完整保留，可切换）；CSS 全面设计 token 化（`--app-*` / `--chart-1..8` 变量 + `.surface` / `.btn` / `.pill` / `.badge-*` / `.skeleton` / `.meter` 等组件类），代码内零硬编码颜色类；共享 UI 原语 `Card` / `Skeleton` / `LoadingRows` / `EmptyState` / `Stat` / `Meter`
- 中/EN 双语切换：头部「中/EN」切换按钮，zh 模式全中文、en 模式全英文（覆盖全部组件含 Settings、桌面小组件、错误边界），选择经 localStorage 持久化
- 新应用图标：暖铜色递增柱状图 + token 圆点 motif（桌面端 `src-tauri/icons/` 全套 + 前端 `favicon.svg`，预览见 `docs/icon-preview.png`）
- 预算告警条（柔和色块）、响应式布局
- WebSocket 实时刷新（扫描完成后自动更新）
- Settings：定价编辑、Collector 开关、自动扫描间隔、开机自启、预算、订阅档位（羊毛价值）、CSV/JSON/PDF 导出、数据保留、CA 证书安装

## Web API

| 端点 | 说明 |
|---|---|
| `GET /api/health` | 健康检查 |
| `GET /api/overview` | 总体统计 |
| `GET /api/providers` | 按 Provider 分组 |
| `GET /api/models` | 按 Model 分组 |
| `GET /api/pricing/models` | 定价模型 + context window 列表 |
| `GET /api/tools` | 按 Tool 分组 |
| `GET /api/tools/ranking` | Agent 工具调用 TOP（从 transcript/raw_json 解析） |
| `GET /api/skills/ranking` | Skill 使用 TOP |
| `GET /api/projects` | 按项目分组（从 source_file/session 路径推断） |
| `GET /api/trends` | 日趋势 |
| `GET /api/heatmap` | Token 半年热力图（默认 180d，零填充） |
| `GET /api/heatmap/hourly` | 星期 × 小时活动热力图（服务器本地时区聚合） |
| `GET /api/requests` | 请求列表（分页+过滤） |
| `GET /api/sessions` | 会话级用量分组 |
| `GET /api/export` | 导出 CSV/JSON/PDF（PDF 为打印就绪 HTML 报表） |
| `POST /api/scan` | 触发采集 |
| `POST /api/ingest` | Webhook 推送 usage 记录（单条或 `{"records":[...]}` 批量，≤1000 条） |
| `POST /api/events/scan-complete` | 扫描完成事件（Tauri 推送 WebSocket） |
| `GET /api/collectors` | Collector 可用状态 |
| `GET/PUT /api/config/budget` | 预算配置 |
| `GET/PUT /api/config/pricing` | 定价覆盖 |
| `PUT /api/config/collectors` | Collector 启用/禁用（返回更新后状态） |
| `GET/PUT /api/config/general` | 通用配置（扫描间隔、开机自启） |
| `GET/PUT /api/config/data` | 数据保留策略 |
| `GET/PUT /api/config/subscription` | 订阅档位（羊毛价值估算） |
| `GET /api/ca/status` | CA 证书安装状态 |
| `POST /api/ca/install` | 安装 CA 到系统信任库 |
| `POST /api/ca/uninstall` | 移除 CA 证书 |
| `GET /api/quota/codex?refresh=true` | Codex 额度（app-server JSON-RPC，可刷新） |
| `GET /api/quota/claude?refresh=true` | Claude 额度（statusLine 快照，可刷新） |
| `GET /api/ws` | WebSocket 扫描完成推送 |

## 主动推送（Webhook + MCP）

除自动扫描本地日志外，工具也可以**主动推送** usage 数据（Layer 3）。

### Webhook：`POST /api/ingest`

`serve` 运行期间，任何脚本/工具都可以 HTTP 推送记录，成本按定价表自动计算（也可自带 `cost_usd`），写入后 Dashboard 经 WebSocket 自动刷新：

```bash
# 单条
curl -X POST http://127.0.0.1:3210/api/ingest \
  -H 'Content-Type: application/json' \
  -d '{"provider":"openai","model":"gpt-4o","input_tokens":1200,"output_tokens":300,"tool":"my-agent"}'

# 批量（≤1000 条）
curl -X POST http://127.0.0.1:3210/api/ingest \
  -H 'Content-Type: application/json' \
  -d '{"records":[{"provider":"deepseek","model":"deepseek-chat","input_tokens":500,"output_tokens":200}]}'
```

字段：必填 `provider`/`model`；可选 `timestamp`(RFC3339)、`tool`、各 token 计数（`total_tokens` 缺省自动求和）、`cost_usd`、`latency_ms`、`is_stream`、`status_code`、`session_id`、`request_id`、`notes`、`raw_json`。入库记录的 `collector` 固定为 `webhook`。

### MCP Server：`alltokens mcp`

以 stdio 运行 Model Context Protocol 服务（手写 JSON-RPC，零新依赖），AI 工具既可查询用量也可推送记录。接入 Claude Code：

```bash
claude mcp add alltokens -- alltokens mcp
```

通用 MCP 客户端配置：

```json
{ "mcpServers": { "alltokens": { "command": "alltokens", "args": ["mcp"] } } }
```

提供的 5 个工具：

| 工具 | 说明 |
|---|---|
| `get_overview` | 总体统计（可选 `last: "7d"`） |
| `get_stats` | 按 provider/model/tool 分组统计 |
| `list_requests` | 请求级记录（过滤 + limit ≤100） |
| `get_budget_status` | 月预算与本月已用百分比 |
| `report_usage` | 推送 usage 记录（collector 记为 `mcp`） |

## Tauri 桌面端

```bash
# 开发模式
cd src-tauri && npx @tauri-apps/cli dev

# 构建（Windows: MSI + NSIS）
cd src-tauri && npx @tauri-apps/cli build
```

桌面端特性：内嵌 Web 服务、系统托盘（Show/Hide/Quit）、关闭到托盘、预算 OS 通知、后台自动扫描、开机自启；托盘 tooltip/title 显示 Codex/Claude 5h/7d 额度（每 2 分钟读缓存刷新，扫描后同步；实时拉取通过 Dashboard `?refresh=true`）；**桌面悬浮小组件**——托盘菜单「桌面小组件」勾选开关，320×480 无边框置顶小窗（今日成本/tokens + Codex/Claude 额度条 + 近 7 天迷你趋势），可拖动（位置持久化，重启还原），× 按钮或托盘勾选隐藏。

## 项目结构

```
alltokens/
├── Cargo.toml                   # workspace
├── crates/
│   ├── core/                    # 数据模型、存储、定价引擎、导出
│   ├── collectors/              # 23 个数据采集器 + WSL 路径支持 + MCP Server (mcp.rs)
│   ├── web/                     # axum Web API + WebSocket + 静态文件
│   ├── proxy/                   # 转发代理 + MITM TLS 解密（Phase 3）
│   └── cli/                     # CLI 工具
├── frontend/                    # React 19 + Tailwind 4 Dashboard
│   └── dist/                    # 构建产物（嵌入 web crate）
├── src-tauri/                   # Tauri 2.x 桌面端
├── pricing/
│   └── builtin.toml             # 55 模型定价表
├── STATUS.md                    # 完成度分析
└── PLAN.md                      # 架构与开发计划
```

## 构建与验证

```bash
cargo test --workspace             # 167 个测试
cd src-tauri && cargo test         # 3 个测试（托盘额度）
cd frontend && npm install && npm run build  # 前端依赖 + 构建
cd src-tauri && npx @tauri-apps/cli build    # 桌面端打包（MSI + NSIS）
# 合计 170 个测试
```

## 技术栈

| 层 | 选型 |
|---|---|
| 核心引擎 | Rust |
| 存储 | SQLite (rusqlite) |
| Web API | axum + WebSocket |
| 前端 | React 19 + Tailwind 4（纯 CSS/SVG 图表） |
| 桌面端 | Tauri 2.x + autostart + notification 插件 |
| 代理 | hyper（转发 + MITM TLS 解密，自签 CA 动态签发） |

## 待完成

- 无既定功能项（Phase 1–4 全部完成）
- 待验证：Kimi CLI 真实日志样本（本机未安装，有样本后可验证 Collector）

## License

MIT
