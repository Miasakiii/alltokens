# Shared Task Notes

> Context bridge for autonomous dev loops. Updated each iteration by the agent.

## Progress

### P0 — Must do

- [x] **Verify Collectors in real environments** — Claude Code 33,567 条真实入库；cc-switch（`proxy_request_logs` schema 漂移修复，8,403 条）/ OpenCode（`message` 表 JSON blob 修复，164 条）真实验证 + 二次扫描 0 重复；Qwen/Trae/Qoder 确认本地无请求级 token 数据（已知限制，同 Cursor）；Kimi CLI 本机未安装待样本
- [x] **Codex Collector 升级 (codexU P0)** — JSONL rollout delta parsing, SQLite coarse fallback, `source_quality` notes, probe CLI, fixtures, app-server quota JSON-RPC, `CodexRateLimitNormalizer`, `GET /api/quota/codex`, Dashboard quota card
- [x] **Tauri build verification** — Windows MSI + NSIS
- [x] **Unit tests for core** — 147 workspace + 3 Tauri = 150 tests, `cargo test` passes

### P1 — Should do

- [x] **Frontend charts & filters** — Provider/Model/Tool breakdown, period selector, request filters
- [x] **WebSocket live refresh** — `useScanComplete` triggers data refetch on scan（现同时暴露 `connected` 状态）
- [x] **Data export** — CLI `export`, `GET /api/export`, Settings CSV/JSON/PDF buttons
- [x] **Claude 额度快照 (codexU P1)** — statusLine snapshot、`GET /api/quota/claude`、`ClaudeQuotaCard`、`alltokens probe claude`
- [x] **Probe 扩展 (codexU P1)** — `alltokens probe` 列出全部采集器；`probe cursor|opencode|windsurf [--json]`；共享 `BasicProbeResult`
- [x] **项目维度聚合 (codexU P1)** — `extract_project_name` + `GET /api/projects` + Dashboard 项目排行
- [x] **Tool/Skill 调用 TOP (codexU P1)** — `invocation` parser + `GET /api/tools/ranking` + `GET /api/skills/ranking` + Dashboard 图表
- [x] **Token 半年热力图 (codexU P2)** — `GET /api/heatmap`（默认 180d）；`fill_heatmap_days` 聚合 + 零填充；Dashboard `TokenHeatmap` 日历格 + tooltip
- [x] **Transparent proxy Phase 3 (95%)** — Forward proxy + intercept + SQLite persist + MITM TLS（自签 CA + 动态叶子证书）+ CA 一键安装（Windows/macOS/Linux）+ chunked/SSE 流式解码 + gzip/deflate/br 解压 + 15 个 Provider host 拦截（含 Gemini `usageMetadata` 解析，JSON/SSE 双路径）
- [x] **Phase 4 Layer 3 主动推送 (✅ 2026-07-18)** — Webhook `POST /api/ingest`（单条/批量 ≤1000，定价自动计算，collector=`webhook`，WS 复用 scan_complete 前端零改动）；MCP Server `alltokens mcp`（collectors/mcp.rs 手写 stdio JSON-RPC 零新依赖，5 工具含 report_usage，stdout 协议通道故 tracing 走 stderr）；web +3 / collectors +4 测试；README/PLAN/STATUS 已同步
- [x] **UI 重设计 + 双语化 (2026-07-18)** — Warm Ledger 设计系统（index.css 全 token 化、primitives.tsx 共享原语、26 个组件/页面重写、零硬编码颜色、浅色默认主题）；中/EN 双语切换（`src/i18n.tsx` + Layout 切换按钮，zh 全中文 / en 全英文，localStorage 持久化）；Dashboard 周期选择器旁具体日期范围标签；新应用图标（src-tauri/icons 全套 + 手写 favicon.svg + scripts/make_icons.py）；构建 70 modules ✅

### P2 — Nice to have

- [x] **Request detail modal** — Row click → modal with tokens, cost, raw JSON, context-window 占用条
- [x] **趋势面板 TrendPanel** — 原 TrendChart/CostTrendChart/CacheHitRateChart 三图合并（metric tabs + Daily/Weekly 切换），顺带修复日数据按 token 量排序的 bug
- [x] **Budget alerts** — Dashboard banner + Tauri OS notifications (80%/100%)
- [x] **Light/dark theme** — `useTheme` + toggle in Layout
- [x] **CLI budget commands** — `budget set|status`
- [x] **Mobile responsive layout** — Tailwind responsive breakpoints in Layout/Dashboard
- [x] **Tauri tray + close-to-tray + notifications**
- [x] **Tauri tray quota display (codexU P2)** — cached `codex_quota`/`claude_quota` snapshots; tooltip + disabled menu header (`5h: 72% | 7d: 45%`); macOS/Linux tray title (`C:72% L:68%`); refresh on scan-complete event + 2 min periodic (cache read only, no app-server spawn); live fetch only via dashboard `GET /api/quota/*?refresh=true`
- [x] **Settings sections** — Pricing, Collectors, General, Budget, Data, Subscription, CA
- [x] **Custom pricing editor UI** — `GET/PUT /api/config/pricing`; USD→CNY + per-model overrides
- [x] **Per-collector enable/disable** — `PUT /api/config/collectors`; scan skips disabled
- [x] **Auto-scan interval** — `GET/PUT /api/config/general`; background loop in `serve` + Tauri
- [x] **Boot autostart** — `launch_at_startup` + `tauri-plugin-autostart`; Settings toggle
- [x] **Data retention** — `DataConfig.retention_days`; purge on save; Settings Data section
- [x] **raw_json in collectors** — 14 collector 源文件写入 raw_json（generic 系列含 Trae/Qoder 等）
- [x] **agentsview 借鉴 · Dashboard UI 优化** — TrendPanel 合并、Dashboard 一键 Scan 按钮、Updated 新鲜度指示、底部 StatusBar（周期汇总 + Live 状态）、过滤器 chips、共享控件 `ui/SegmentedControl` + `ui/FreshnessLabel`、卡片 `.surface` 词汇统一 + 密度收紧、删除死代码 App.css
- [x] **桌面悬浮小组件 (Phase 4 P2 ✅ 2026-07-18)** — Tauri 第二个 `WebviewWindow`（320×480 frameless/alwaysOnTop/skipTaskbar，默认隐藏），`index.html?widget=1` 分流渲染紧凑 `WidgetView`（今日成本 + Codex/Claude 5h/7d 额度条 + 近 7 天迷你趋势，drag-region 拖动）；core `WidgetConfig{visible,x,y}` 存 app_config（镜像 BudgetConfig），Moved 事件持久化位置、启动还原显隐+位置；托盘「桌面小组件」CheckMenuItem 三向同步（托盘/组件内 ×/启动）；`withGlobalTauri` + 新 command `set_widget_visible`/`open_main_window`（零新 JS 依赖）；**顺带修复 Tauri 生产窗口同源 /api 不通的存量 bug**（client.ts/useWebSocket 检测 tauri 源指向 127.0.0.1:3212，主窗口 Dashboard 桌面端首次真正可用）；core +1 测试（widget_config_round_trip）

### P3 — Cleanup

- [x] **前端全局错误边界** — `ErrorBoundary` 包裹 `<App/>`，渲染异常 → `.surface` 恢复卡片 + Reload 按钮
- [x] **持久化应用日志** — `serve`/`daemon` tracing 日志 tee 到 `<db_dir>/logs/alltokens.log`（>5MB 启动截断，零新依赖）；cli `logging.rs` +2 测试

## Next Steps

1. **P0** — Kimi CLI 真实日志样本（本机未安装）；Qwen/Trae/Qoder 已确认本地无请求级 token 数据（已知限制）
2. **P1–P2 backlog 已全部清空** — Phase 1–4 既定目标完成（最后一块「桌面悬浮小组件」2026-07-18 落地）；后续方向候选：桌面端手动验证清单（拖拽/置顶/托盘切换/重启还原）、新 Collector 按需增补、macOS/Linux 桌面端构建验证

## codexU 借鉴参考

> 分析对象: https://github.com/shanggqm/codexU (macOS Swift 菜单栏 Widget，专注 Codex + Claude Code)

**值得借鉴（按优先级）**

- **P0 — Codex Collector 升级**: ✅ Done — reads `rollout-*.jsonl` + `archived_sessions/*.jsonl` with cumulative delta algorithm; `state_5.sqlite` coarse fallback; `notes=source_quality:detailed|coarse`; `alltokens probe codex [--json]` with quota section; app-server `account/rateLimits/read`; `CodexRateLimitNormalizer` by `windowDurationMins` (300/10080); `GET /api/quota/codex?refresh=true`; cached in `app_config` key `codex_quota`.
- **P1 — 额度窗口归一化**: ✅ Done in P0 — `CodexRateLimitNormalizer` 按 `windowDurationMins` 分类，不依赖 primary/secondary 槽位顺序。
- **P1 — Claude 额度快照**: ✅ Done — reads statusLine snapshot cache (`{cache}/codexU/claude-code/statusline-snapshot.json` on macOS/Linux/Windows, plus `~/.claude/statusline-snapshot.json` and `/tmp/claude/statusline-raw.json`); `ClaudeStatusLineNormalizer` for 5h/7d `used_percentage`; `GET /api/quota/claude?refresh=true`; cached in `app_config` key `claude_quota`; Dashboard `ClaudeQuotaCard`; `alltokens probe claude [--json]`.
- **P1 — 数据质量元数据**: 区分 official / local / estimated 口径；缺失显示 `--` 而非伪 0；probe 已实现 codex、claude、cursor、opencode、windsurf；`alltokens probe` 无参数列出全部检测状态。
- **P1 — 分析维度扩展**: 项目排行 ✅；Tool/Skill TOP ✅；半年热力图 ✅ — `GET /api/heatmap`、Dashboard `TokenHeatmap`；今日任务看板 ✅ — `TodaySummary`（今日/本周/本月 API 等效成本 tile）。
- **P2 — 订阅价值估算**: ✅ Done — 订阅档位配置（`GET/PUT /api/config/subscription`）+ `TodaySummary`「本月回本 X%」进度条。
- **P2 — Tauri tray 额度展示**: ✅ Done — tooltip + disabled menu header from cached `codex_quota`/`claude_quota`; macOS/Linux `set_title` compact 5h (`C:72% L:68%`); 2 min cache-read refresh + scan-complete event hook; no per-tick app-server spawn.

**AllTokens 已更强**: 23 Collector 跨平台、16 Provider 定价、CLI/Web/Tauri、预算告警、导出、透明代理、cc-switch 导入。

## agentsview 借鉴参考 (2026-07-17)

> 分析对象: https://github.com/kenn-io/agentsview (4.4k★, Go + Svelte 5, 本地优先会话搜索/用量分析)

**已落地**: 信息密度收紧（卡片 p-5→p-4）、`TrendPanel` metric tabs（Tokens/Cost/Cache hit + Daily/Weekly）、Dashboard 一键 Scan 按钮（`POST /api/scan` 此前无 UI 入口）+ "Updated X ago" 新鲜度、底部 `StatusBar`（周期汇总 + WebSocket Live 状态 + 更新时间）、过滤器 chips 单独清除、共享控件词汇（`ui/SegmentedControl`、`ui/FreshnessLabel`）、卡片 `.surface` 类统一（13 处硬编码替换）。

**已落地（2026-07-17 第二轮）**: Hour-of-week 热力图——core `get_hour_of_week`（`strftime %w/%H + localtime`，刻意本地时区以呈现活动节律）+ `GET /api/heatmap/hourly` + `HourOfWeekHeatmap`（7×24 网格、emerald 强度、时区标注）；core +1、web +1 测试。

**不照搬**: 会话全文搜索/三栏会话浏览器（其本质是 transcript 浏览器，数据模型不同）、i18n（paraglide 5 语言，收益低改动大）、PG/DuckDB/Quack/S3 多后端（团队向）、MCP server / embeddings（Phase 4 候选）。

## Notes

- **Test count:** 170 tests — workspace 167 (`cargo test --workspace`) + Tauri tray 3 (`cd src-tauri && cargo test`)
- **Verify commands:** `cargo test --workspace`, `cd frontend && npx tsc -b && npm run build`, `cd src-tauri && npx @tauri-apps/cli build`
- **Code size:** ~12,400 lines Rust, ~4,200 lines frontend TS/TSX
- **Collectors:** 23 registered in `register_collectors()`
- **`daemon` CLI subcommand 已上线** — 常驻定时扫描 + 按保留天数自动归档（`--interval` 分钟 / `--once` 单周期）
- **Phase 4 全部完成** — Webhook `POST /api/ingest` + MCP Server `alltokens mcp` + 桌面悬浮小组件（托盘「桌面小组件」开关，`WidgetConfig` 持久化显隐+位置）
- **Do not** auto-commit unless explicitly requested
- **STATUS.md** — Concise completion analysis (updated 2026-07-18)
