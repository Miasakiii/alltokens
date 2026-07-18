# AllTokens 项目完成度分析

> 最后更新: 2026-07-18

## 一、项目概况

| 指标 | 数值 |
|---|---|
| Rust 代码 | ~12,400 行（Phase 4 新增 MCP Server ~470 行 + Webhook ingest ~140 行 + 小组件 ~180 行） |
| 前端代码 | ~5,400 行（本轮 UI 重设计 + 双语化；此前新增 TrendPanel 趋势面板合并 + StatusBar 状态栏 + SegmentedControl/FreshnessLabel 共享控件 + 过滤器 chips + Dashboard 一键 Scan + `.surface` 样式收敛 + WidgetView 小组件视图；删除 3 个旧趋势图组件与死代码 App.css） |
| 编译状态 | ✅ 零 error / 零 warning |
| Collector 数量 | 23 |
| 定价模型 | 55（16 Provider） |
| 单元/集成测试 | 167（CLI 2 + collectors 35+12+5 + core 54 + proxy 41 + web 18，全部通过） |
| 前端构建 | ✅ 70 modules · JS 296 kB → 88 kB gzip · CSS 29 kB → 7 kB gzip |
| Tauri 构建 | ✅ Windows MSI + NSIS |

**整体完成度：约 98%**（Phase 1–4 既定目标全部完成：Core/CLI/Collectors → Web Dashboard + Tauri → 透明代理 MITM → 主动推送层（Webhook + MCP Server）→ 桌面悬浮小组件；顺带修复 Tauri 生产窗口 API 源不通的存量 bug。剩余仅为 P0 验证项：Kimi CLI 真实日志样本，本机未安装）

## 二、各模块完成度

### ✅ Phase 1: Core + CLI (完成度 99%)

| 组件 | 状态 |
|---|---|
| 数据模型、存储、定价、Schema | ✅（含 `reasoning_tokens` 字段 + `ALTER TABLE` 迁移） |
| CLI (init/scan/today/list/stats/cost/pricing/serve/export/budget/proxy/probe) | ✅ |
| 数据导出 CSV/JSON/PDF 报表（CLI + API + Settings UI） | ✅ |
| 预算配置与告警（CLI + API + Dashboard + Tauri OS 通知） | ✅ |
| Overview 统计含 `total_reasoning_tokens` | ✅ |

**持久化应用日志 (P3 ✅):** cli 新增 [logging.rs](file:///f:/su/alltokens/crates/cli/src/logging.rs)——`serve`/`daemon` 子命令启动时将 tracing 日志 tee 到 `<db_dir>/logs/alltokens.log`（stdout 保留；启动时文件 >5MB 则截断；`MakeWriter` tee 实现，零新依赖）；`Cli::parse` 前移至日志初始化之前，按子命令决定是否挂文件输出；CLI +2 测试（tee 双写、路径推导）。

**缺失:** 桌面小组件（Phase 4 P2，方案见 PLAN.md Phase 4）

### ✅ Phase 1.5: Collectors (完成度 97%)

23 个 Collector 注册；fixture 测试覆盖 Kimi/Qwen/Trae/Qoder/Claude/Cursor 等；多数 Collector 写入 `raw_json`。

**Codex 升级 (codexU P0 ✅):** `rollout-*.jsonl` + `archived_sessions` 累计 delta；`state_5.sqlite` 粗粒度回退；`notes=source_quality:detailed|coarse`；app-server `account/rateLimits/read`；`CodexRateLimitNormalizer`（5h/7d）；`alltokens probe codex [--json]`；提取 `delta.reasoning_output_tokens` 写入 `reasoning_tokens` 字段。

**Claude 额度 (codexU P1 ✅):** statusLine 快照（多路径）；`ClaudeStatusLineNormalizer`；`alltokens probe claude [--json]`。

**Probe 扩展 (codexU P1 ✅):** `alltokens probe` 列出全部采集器；`probe cursor|opencode|windsurf [--json]`；修复 tokio runtime 嵌套 panic（`Handle::try_current()` + `block_in_place()`）。

**真实环境验证 (P0 ✅):** Claude Code 2025+ UUID 会话格式解析（`ClaudeSessionLine`/`ClaudeMessage`/`ClaudeMessageUsage`）；`real_env_validation.rs` 5 个集成测试；33,567 条真实记录端到端入库。Cursor `ai-code-tracking.db` 经确认仅存代码归因数据（无 token 用量，属已知限制）。

**真实环境验证第二轮 (P0 ✅, 2026-07-18):** 本机 probe 13 个工具可用；临时库全量扫描发现 cc-switch / OpenCode「检测到却 0 记录」的 schema 漂移 bug。[cc_switch.rs](file:///f:/su/alltokens/crates/collectors/src/cc_switch.rs) 修复：①表名候选补 `proxy_request_logs`（新版 cc-switch 真实表，8,403 行）②时间戳支持 INTEGER epoch 秒/毫秒（原实现对数字回退为"现在"）③成本 TEXT→f64 ④`is_streaming` INTEGER 1/0 ⑤`provider_id` 为 `_session` 占位符时回退模型识别 ⑥`since` 增量过滤按列声明类型传 epoch（原实现 TEXT 比较恒真 → 每次全量重复导入）⑦首扫分页拉全量（原 LIMIT 5000 截断丢历史）；[opencode.rs](file:///f:/su/alltokens/crates/collectors/src/opencode.rs) 新增 `message` 表解析（OpenCode≥0.x：data JSON blob 内 `tokens{input/output/reasoning/cache.read/write}` + `cost` + `modelID`/`providerID` + `time.created` 毫秒 + session_id，reasoning 写入 `reasoning_tokens`）+ 同步分页。各 +1 真实形态 fixture 测试。真实环境验证：首扫 35,340 条（cc-switch 8,403 / OpenCode 164），二扫两采集器 0 重复。Qwen CLI（`~/.qwen` 无会话数据）、Trae（ai-agent 日志无 token 字段且已登出）、Qoder（logs 仅 context-window `usedTokens` 占用快照，无请求级 token）经确认本地无请求级 token 数据，与 Cursor 同类已知限制。

**缺失:** 二级 Collector（Zed/Windsurf/Cline 等）的 reasoning tokens 采集（上游格式尚未暴露）

### ✅ Phase 2a: Web API (完成度 100%)

REST 端点完整；`POST /api/scan`、`POST /api/ingest`（Webhook 推送，见 Phase 4）、`POST /api/events/scan-complete`、WebSocket `/api/ws`、`GET/PUT` 配置端点（budget/subscription/pricing/general/data）、`PUT /api/config/collectors`、`GET /api/export`（format=csv/json 下载附件；format=pdf 返回打印就绪 HTML 报表，浏览器渲染后自动触发打印对话框「另存为 PDF」）、`GET /api/collectors`；codexU 新增：`GET /api/quota/codex`、`GET /api/quota/claude`、`GET /api/projects`、`GET /api/tools/ranking`、`GET /api/skills/ranking`、`GET /api/heatmap`（默认 180d）；`GET /api/heatmap/hourly`（星期 × 小时活动聚合，`strftime %w/%H + localtime` 服务器本地时区分组，与 UTC 日聚合互补）；CA 管理：`GET /api/ca/status`、`POST /api/ca/install`、`POST /api/ca/uninstall`（复用 `alltokens_proxy::{install,uninstall,status,CertificateAuthority}`，`spawn_blocking` 包裹 certutil/security）；定价：`GET /api/pricing/models`（列出全部条目含 `context_window`，供前端算上下文窗口占用 %）；会话：`GET /api/sessions`（按 session_id + provider + model 聚合的会话级统计，最近活跃倒序，上限 200）；`OverviewStats` 现含 `total_reasoning_tokens`。

### ✅ Phase 2b: React Dashboard (完成度 99%)

Stats 卡片、Token/成本趋势图、`TokenHeatmap` 半年热力图、Provider/Model/Tool 排行、`ProjectBreakdown` 项目排行、`ToolInvocationBreakdown` / `SkillInvocationBreakdown` 调用 TOP、缓存命中率图、请求表 + 详情弹窗（含 raw JSON）、搜索过滤、预算告警条、`CodexQuotaCard` / `ClaudeQuotaCard` 额度卡片（缺失显示 `--`）、深浅主题、响应式布局、Settings 全功能。

**Reasoning tokens 展示 (P1 ✅):** 共享 `formatTokens` 格式化（K/M/B 分级）；`ReasoningBadge` 组件；`RequestTable` Output 单元格叠加琥珀色 badge；`RequestDetailModal` 新增字段；`StatsCards` Total Tokens 卡片显示 reasoning 汇总。

**今日看板 + 订阅羊毛价值 (P1 ✅):** [TodaySummary](file:///f:/su/alltokens/frontend/src/components/TodaySummary.tsx) 组件置于 Dashboard 顶部，🐑 卡片 3 tile 展示 今日 / 本周（Monday-start） / 本月 的 API 等效累计成本 + tokens + 请求数，订阅制下即已省下的金额。零后端改动，复用 `/api/overview?start_date=<ISO>`；[utils/dates.ts](file:///f:/su/alltokens/frontend/src/utils/dates.ts) 集中封装 `todayStartISO` / `weekStartISO` / `monthStartISO`；顺带修 `useStats.filterDeps` 未跟踪 `start_date/end_date` 的 refetch 依赖 bug。

**订阅档位 → 已省 X% (P1 ✅):** 新增 `SubscriptionConfig`（core/model.rs `SubscriptionTier{label,monthly_usd}` + `enabled`，存 `app_config` key=`subscription`，镜像 `BudgetConfig`）+ `GET/PUT /api/config/subscription`；Settings 新增 Subscription 区（enable 开关 + 快捷预设 Claude Pro/Max、Codex Plus/Pro、ChatGPT Plus + 自定义档位增删 + 月费合计）；[TodaySummary](file:///f:/su/alltokens/frontend/src/components/TodaySummary.tsx) 在启用且月费>0 时渲染「本月回本 X%」进度条（`monthCost/feeTotal`，≥100% 绿色「已回本·多省 $X」，否则「距回本还差 $Y」）；新增 `useSubscription` hook + storage `subscription_config_round_trip` 测试。

**Context-window % 展示 (P2 ✅):** `pricing/builtin.toml` 为 55 模型补 `context_window` 字段（OpenAI 128K/1M、Claude 200K、Gemini 1M、Qwen-long 10M 等）；core `PricingEntry`/`ModelPricing` 加 `context_window` + `PricingEngine::context_window(provider,model)` 查询；新增 `GET /api/pricing/models` 端点；前端 [usePricingModels](file:///f:/su/alltokens/frontend/src/hooks/usePricingModels.ts) hook 构建 `provider/model → 窗口` 映射（精确 + 后缀回退），[RequestDetailModal](file:///f:/su/alltokens/frontend/src/components/RequestDetailModal.tsx) 渲染每请求「Context window」占用进度条（`total_tokens/context_window`，≥90% 红 / ≥70% 琥珀 / 否则绿）；core +2、web +1 测试。

**会话级视图 (P2 ✅):** 借鉴 codex-token-hud 的 session grouping——core `SessionStats`（session_id/provider/model/collector + tokens/cost + first_seen/last_seen/duration_secs）+ `Storage::get_session_stats`（按 `session_id + provider + model` 分组，排除空 session_id，`MAX(timestamp)` 倒序，LIMIT 200，`duration_between` 解析 RFC3339 算时长）+ `GET /api/sessions` 端点（复用 `StatsQuery` 过滤）；前端 `useSessions` hook + [SessionBreakdown](file:///f:/su/alltokens/frontend/src/components/SessionBreakdown.tsx) 表格（Session/Model/Reqs/Tokens/Cost/Duration/Last active，按最近活跃排序，长 session_id/路径取尾段）挂于 Dashboard；core +1、web +1 测试。顺手补修 prior context_window 任务漏改的 `Settings.emptyPricingEntry` 字面量（缺 `context_window`，esbuild 不做类型检查故潜伏，现 `tsc -b` 零错）。

**PDF 报表导出 (P2 ✅):** 零依赖方案——core [export::to_html_report](file:///f:/su/alltokens/crates/core/src/export.rs) 生成独立打印就绪 HTML 报表（汇总卡片 Requests/Tokens/Input·Output/Cost + 按 provider/model 分组表格 + `@page`/`@media print` CSS + `window.print()` 自动触发；HTML 转义防注入）；经三通道复用——CLI `export --format pdf`（写 HTML 文件供浏览器打印）、`GET /api/export?format=pdf`（inline `text/html`）、Settings「Export PDF」按钮（`window.open` 新标签页渲染后自动弹打印对话框→「另存为 PDF」）。规避 jsPDF/Rust PDF 引擎的新依赖（沙盒禁 npm/crates 拉取）；core +1 测试（含转义断言）。

**fmt 重复函数收敛 (P3 ✅):** 10 个组件（Tool/Model/Project/Provider/Skill/ToolInvocation Breakdown + TrendChart/CostTrendChart/CacheHitRateChart/TokenHeatmap）各自的 K/M/B token 格式化函数（`fmt`/`fmtTokens`，体一致）统一收敛到 [utils/format.ts](file:///f:/su/alltokens/frontend/src/utils/format.ts) 的 `formatTokens`（顺带获得 B 层级 + 非正数护栏）；另新增 `formatInt`（精确千分位）并迁移 `RequestDetailModal` 的本地 `fmt`（`toLocaleString`）与 `ReasoningBadge` 内联调用。`CostTrendChart.fmtCost`（轴标专用分档，非重复）保留。`tsc -b` 零错（noUnusedLocals 验证无残留导入），包体微缩。

**daemon 子命令 + 自动归档 (P3 ✅):** CLI 新增 `alltokens daemon`（`--interval` 分钟 / `--once` 单周期退出）——常驻循环每周期执行 [run_maintenance_cycle](file:///f:/su/alltokens/crates/web/src/scan.rs)：先 `run_scan` 采集全部启用 collector，再按 `DataConfig.retention_days` 调 `purge_records_older_than_days` 自动清理旧记录（0=保留全部），并 `notify_running_servers` 通知在运行的仪表盘刷新。interval 解析优先级：`--interval` > `general.auto_scan_interval_minutes` > 15 分钟回退；`tokio::select!` + `ctrl_c` 优雅停机。逻辑抽到 web crate 的 `run_maintenance_cycle`/`MaintenanceResult` 便于测试；web +2 测试（purge 命中 + retention=0 全保留），`daemon --once` 冒烟验证通过。

**遗留 bug 修复：PricingEngine::find 模糊匹配 (✅):** [pricing.rs](file:///f:/su/alltokens/crates/core/src/pricing.rs) 去版本号回退原用 `model.rsplit('-').skip(1).join("-")`，会把段序反转（`claude-sonnet-4-20250514` → `4-sonnet-claude`）导致带日期后缀的模型永远回退失败、成本算为 0。改用 `rsplit_once('-')` 仅剥离最后一段并保持顺序（→ `claude-sonnet-4`）；core +1 回归测试复现并验证。

**agentsview 借鉴 · Dashboard UI 优化 (P2 ✅):** 参考 [agentsview](https://github.com/kenn-io/agentsview)（4.4k★，Go + Svelte 5 同类本地优先工具）的 PRODUCT/DESIGN 文档与界面。① 修复 `TrendChart` 日数据按 token 量排序（非时间序列）的 bug；② 三张窄趋势图（Token/Cost/CacheHitRate）合并为 [TrendPanel](file:///f:/su/alltokens/frontend/src/components/TrendPanel.tsx) 宽面板——metric tabs（Tokens/Cost/Cache hit）+ Daily/Weekly 粒度切换，tooltip 带日期标签；③ 新增共享控件 [ui/SegmentedControl](file:///f:/su/alltokens/frontend/src/components/ui/SegmentedControl.tsx) 与 [ui/FreshnessLabel](file:///f:/su/alltokens/frontend/src/components/ui/FreshnessLabel.tsx)（daily/weekly 切换此前在两个组件重复实现）；④ Dashboard 头部新增一键 Scan 按钮（`POST /api/scan` 端点早已存在但 UI 无入口，Settings 文案提及的 "Run a scan from the dashboard" 落地）+ "Updated X ago" 新鲜度指示；⑤ 底部 [StatusBar](file:///f:/su/alltokens/frontend/src/components/StatusBar.tsx)（周期汇总 + WebSocket Live 状态 + 更新时间；`useScanComplete` 现返回 `connected`，Layout 新增 `footer` 插槽与底部留白）；⑥ 请求过滤器已激活条件渲染为可单独清除的 chips；⑦ 卡片样式统一收敛到 `.surface` 类（13 处硬编码 `bg-slate-800/60 … rounded-2xl` 替换，light theme 走 CSS 变量更稳）+ 密度收紧（卡片 p-5→p-4）；⑧ 删除死代码 `App.css`（Vite 模板残留，无引用）。`tsc -b` 零错，构建 65 modules · 276 kB → 79 kB gzip。

**Hour-of-week 热力图 (agentsview P2 候选 ✅):** core `HourOfWeekCell` + `Storage::get_hour_of_week`（`strftime('%w'/'%H', timestamp, 'localtime')` 按服务器本地时区聚合——与日粒度 UTC 分组刻意不同，活动节律视图只在用户自身时区有意义；server 与浏览器同机）+ `GET /api/heatmap/hourly`（复用 `StatsQuery` 过滤）；前端 [HourOfWeekHeatmap](file:///f:/su/alltokens/frontend/src/components/HourOfWeekHeatmap.tsx)（7×24 网格、emerald 5 级强度区分 indigo 日热力图、每 3h 刻度、tooltip、Less/More 图例、`UTC±X` 时区标注）+ `useHourOfWeek` hook，与 `TokenHeatmap` 并排列于 `xl:grid-cols-2` 行；core +1（`chrono::Local` 动态期望，机器时区无关）、web +1 测试（core 50 / web 15）。

**全局错误边界 (P3 ✅):** 新增 [ErrorBoundary](file:///f:/su/alltokens/frontend/src/components/ErrorBoundary.tsx) 类组件（`getDerivedStateFromError` + `componentDidCatch` 打 console.error），`main.tsx` 包裹 `<App/>`；任一后代渲染抛错时显示 `.surface` 恢复卡片（错误消息 + Reload 按钮）而非白屏。

**UI 重设计「Warm Ledger」+ 中/EN 双语 (✅ 2026-07-18):** 前端整体设计系统重做——暖色系低饱和「Warm Ledger」风格，浅色为默认主题（深色完整保留）。① [index.css](file:///f:/su/alltokens/frontend/src/index.css) 全部 token 化：CSS 变量 `--app-*` / `--chart-1..8` + 组件类 `.surface`/`.surface-2`/`.btn`/`.icon-btn`/`.pill`/`.input`/`.badge-*`/`.skeleton`/`.meter`/`.label-xs`/`.num`；新增 [ui/primitives.tsx](file:///f:/su/alltokens/frontend/src/components/ui/primitives.tsx) 原语（Card/Skeleton/LoadingRows/EmptyState/Stat/Meter）；SegmentedControl 与 Layout 重写；全部 26 个组件/页面重写，代码内零硬编码颜色类。② 中/EN 双语切换：新增 [i18n.tsx](file:///f:/su/alltokens/frontend/src/i18n.tsx)（LanguageProvider + `useLang` hook，localStorage key `alltokens-lang`，默认 zh），Layout 头部中/EN 切换按钮；zh 模式 100% 中文、en 模式 100% 英文，覆盖全部组件含 Settings、WidgetView、ErrorBoundary。③ Dashboard 周期选择器旁新增具体日期范围标签（0d 显示今天日期；7d/30d/90d 显示起止日期，跨年补年份），双语。④ 新应用图标全套：暖铜色递增柱状图 + token 圆点，`src-tauri/icons/` 全套（32x32.png、128x128.png、icon.png 512、icon.ico 多尺寸）+ 手写 [favicon.svg](file:///f:/su/alltokens/frontend/public/favicon.svg) + 展示图 `docs/icon-preview.png` + 处理脚本 `scripts/make_icons.py`。⑤ 其它 UI 变化：分布面板 Top 8 + 展开收起；配额卡状态着色（剩余 ≥50% 绿 / ≥20% 黄 / <20% 红）+ 重置时间显示 + Codex rate-limited 徽章；BudgetAlert 柔和色块；占比条改为真实占比。生产构建 70 modules，JS 295.62 kB→87.77 kB gzip，CSS 29.41 kB→6.73 kB gzip（原 48.8 kB，移除旧浅色 hack）。

**缺失:** 独立 Analytics/Requests 页面（功能已集成在 Dashboard）

### ✅ Phase 2c: Tauri 桌面端 (完成度 95%)

内嵌 Web 服务、系统托盘（Show/Hide/Quit）、关闭到托盘、预算 OS 通知（80%/100%）、后台自动扫描、开机自启（`tauri-plugin-autostart`）、Windows 构建验证（MSI + NSIS）、桌面悬浮小组件（见 Phase 4）。

**托盘额度 (codexU P2 ✅):** tooltip 显示 Codex/Claude 5h/7d 剩余 %；macOS/Linux compact title；每 2 分钟读缓存刷新 + 扫描完成后同步；无缓存时提示打开 Dashboard 刷新。

**Tauri 生产窗口 API 修复 (✅ 2026-07-18):** 前端此前一律同源 `/api` + 同源 WS——`serve`/vite-dev 下正常，但 Tauri 生产窗口源为 `http://tauri.localhost`（静态资源协议），fetch/WS 实际不通（主窗口 Dashboard 在桌面端一直是空数据，构建验证只覆盖了「能打包」未覆盖「能取数」）。[client.ts](file:///f:/su/alltokens/frontend/src/api/client.ts) 现检测 Tauri 源（`tauri:` 协议或 `tauri.localhost` 主机名）并将全部 API 调用指向内嵌服务 `http://127.0.0.1:3212`（CORS permissive 早已就位），[useWebSocket](file:///f:/su/alltokens/frontend/src/hooks/useWebSocket.ts) 同步改指 `ws://127.0.0.1:3212/api/ws`；桌面端主窗口首次真正可用，也是小组件数据通路的前提。

**缺失:** 无已知大项

### 🔧 Phase 3: 透明代理 (完成度 95%)

HTTP 转发 + CONNECT 隧道、OpenAI/Anthropic/Qwen/SSE usage 拦截、CLI `proxy start [--mitm --ca-dir]`、代理写入 SQLite、CLI `ca install/uninstall/status/path`、桌面端 CA 安装/卸载按钮（Settings 页走 web `/api/ca/*`）、chunked/SSE 流式响应解码、Content-Encoding（gzip/deflate/br）解压。

**MITM TLS (P1 ✅):** [ca.rs](file:///f:/su/alltokens/crates/proxy/src/ca.rs) 303 行 —— 自签根 CA 生成/加载（PEM），LRU 缓存的逐主机叶子证书动态签发；[mitm.rs](file:///f:/su/alltokens/crates/proxy/src/mitm.rs) 362 行 —— 双向 TLS（客户端伪装 + 真实上游连接），HTTP 请求/响应解密并复用 `intercept.rs` 提取 usage（含 `completion_tokens_details.reasoning_tokens`）；17 个 proxy 测试全过。

**CA 一键安装 (P1 ✅):** [ca_install.rs](file:///f:/su/alltokens/crates/proxy/src/ca_install.rs) —— 跨平台把自签 CA 装入/移除系统信任库（Windows `certutil -addstore -user Root`、macOS `security add-trusted-cert` 写 login keychain、Linux `update-ca-certificates`），按 CN `AllTokens MITM CA` 匹配做 uninstall/status，无哈希依赖；CLI `alltokens ca install|uninstall|status|path`（`--ca-dir` 复用 `ProxyConfig::ca_dir`）；6 个参数构建/状态映射单测。

**chunked/SSE 流式解码 (P1 ✅):** [mitm.rs](file:///f:/su/alltokens/crates/proxy/src/mitm.rs) 新增 `dechunk` 解码 HTTP/1.1 分块传输（十六进制块大小 + 忽略 chunk-extension）+ `body_for_extraction` 统一出口，chunked JSON 不再因分块框架解析失败；[intercept.rs](file:///f:/su/alltokens/crates/proxy/src/intercept.rs) 的 `extract_usage_from_sse` 保真 `reasoning_tokens`（`completion_tokens_details.reasoning_tokens`）与 `total_tokens`，流式请求不再丢 reasoning。

**Content-Encoding 解压 (P1 ✅):** [mitm.rs](file:///f:/su/alltokens/crates/proxy/src/mitm.rs) 新增 `content_encoding` + `decompress`，在 `body_for_extraction` 内于 dechunk 之后按 `Content-Encoding` 解压 gzip/x-gzip（flate2）、deflate（zlib→raw 回退）、br（brotli），identity/未知原样透传；best-effort（解压失败回退原始字节，绝不 panic），补齐 Cloudflare 压缩响应的 usage 提取；新增 8 个测试（proxy 30→38）。

**桌面端 CA 安装按钮 (P1 ✅):** web crate 新增 `alltokens-proxy` 依赖 + `/api/ca/{status,install,uninstall}` 三端点（`CaStatusPayload{status,cert_present,cert_path,platform}`，阻塞的 certutil/security 调用经 `spawn_blocking` 包裹，失败→500）；前端 `useCa` hook + [Settings.tsx](file:///f:/su/alltokens/frontend/src/pages/Settings.tsx) 新增「HTTPS 拦截证书 (CA)」区（状态徽章 / cert_path 展示 / 安装·移除按钮 + busy 禁用 / 平台提示 / error 文案）；与 CLI `ca install` 完全复用同一 `ca_install` 逻辑，同时覆盖桌面 App 与 `alltokens serve`；web crate 新增 3 测试（7→10）。

**更多 endpoint 覆盖 (P1 ✅, 2026-07-18):** [intercept.rs](file:///f:/su/alltokens/crates/proxy/src/intercept.rs) 拦截 host 从 8 个扩到 15 个（新增 `generativelanguage.googleapis.com`、`api.x.ai`、`api.groq.com`、`api.mistral.ai`、`api.stepfun.com`、`api.baichuan-ai.com`、`api.lingyiwanwu.com`，覆盖全部 16 个定价 Provider 的 API 域名）；新增 Google Gemini `usageMetadata` 格式解析（`promptTokenCount`/`candidatesTokenCount`/`cachedContentTokenCount`/`thoughtsTokenCount`/`totalTokenCount` + `modelVersion` 回退 model_hint），JSON body 与 SSE 流式双路径贯通，SSE 路径同步补齐 cache 字段提取；proxy +3 测试（38→41）。

**缺失:** 无已知大项（MITM / CA / 流式解码 / 全 Provider endpoint 均已上线）

### ✅ Phase 4: 主动推送层 (Layer 3 完成，2026-07-18)

**Webhook 推送 (✅):** web crate 新增 `POST /api/ingest`——接受单条对象或 `{"records":[...]}` 批量（≤1000 条/次，超出 400）；仅 `provider`/`model` 必填（缺失条目跳过计数），`timestamp` 缺省取当前时间，`total_tokens` 缺省求和四类 token，`cost_usd` 缺省走 `PricingEngine::calculate_cost`（自带成本保留并换算 CNY）；`collector` 强制 `webhook`（不信任客户端自报来源）；插入后复用 `emit_scan_complete` WS 通道，前端零改动自动刷新；web +3 测试（15→18）。

**MCP Server (✅):** [mcp.rs](file:///f:/su/alltokens/crates/collectors/src/mcp.rs) 470 行——手写最小 MCP 协议（NDJSON over stdio，零新依赖，不引入 `rmcp`）：`initialize`（protocolVersion `2024-11-05`）/ `ping` / `tools/list` / `tools/call`，通知（无 id / id null）静默、解析失败 -32700、未知方法 -32601、未知工具 -32602（MCP 规范口径）；核心为纯函数 `McpServer::handle_message(&str) -> Option<String>` 便于单测，`run_stdio` 为薄 IO 循环。5 个工具：`get_overview` / `get_stats{by}` / `list_requests`(≤100) / `get_budget_status`（月预算 + 本月已用 %）/ `report_usage`（推送入库，collector=`mcp`，raw_json 存原始参数，定价自动计算）。CLI 新增 `alltokens mcp` 子命令；**stdout 为协议通道**，tracing 经 `logging::init_stderr` 改走 stderr。collectors +4 测试（initialize / tools-list / report_usage 全链路 / 协议错误与通知）。

**多设备同步（已实现）:** `sync export`（WAL checkpoint 后复制一致快照）/ `sync import`（`Storage::merge_from` 按唯一键幂等去重合并）早已上线，本阶段无需重做。

**桌面悬浮小组件 (P2 ✅):** Tauri 第二个 `WebviewWindow`（label=`widget`，320×480，frameless / alwaysOnTop / skipTaskbar / 不可缩放，默认隐藏），`WebviewUrl::App("index.html?widget=1")` 加载同包前端；[App.tsx](file:///f:/su/alltokens/frontend/src/App.tsx) 按 `?widget=1` 分流到紧凑视图 [WidgetView](file:///f:/su/alltokens/frontend/src/components/WidgetView.tsx)（今日成本/tokens/请求数 + Codex/Claude 5h/7d 额度条 + 近 7 天补零柱状迷你趋势，复用 `useOverview`/`useTrends`/`useCodexQuota`/`useClaudeQuota`/`useScanComplete` hooks，`data-tauri-drag-region` 拖动）；`tauri.conf.json` 开 `withGlobalTauri`，× 按钮 / 「打开 Dashboard」经 `window.__TAURI__.core.invoke` 调新增的 `set_widget_visible` / `open_main_window` 两个 command（零新 JS 依赖）；core `WidgetConfig{visible,x,y}` 存 `app_config` key=`widget`（镜像 `BudgetConfig`），`WindowEvent::Moved` 持久化物理坐标、启动时按配置还原显隐与位置；托盘菜单新增「桌面小组件」`CheckMenuItem`，显隐状态三向同步（托盘切换 / 组件内 × / 启动还原）。core +1 测试（`widget_config_round_trip`，core 53→54）；完整 `tauri build` 验证通过（2026-07-18：release 编译 2m50s，MSI 7.3M + NSIS 5.1M）。前端新增 devDependency `@tauri-apps/cli@2.11.4`（构建工具链，非运行时依赖）。

**遗留:** 无（Phase 4 全部完成）

## 三、技术债务

| 问题 | 严重度 |
|---|---|
| 二级 Collector 的 reasoning tokens 需上游格式更新 | 🟢 低 |

## 四、优先级 backlog

### P1（全部完成 ✅）
_P1 backlog 已清空（最后一项「桌面端 CA 安装按钮」已完成）。_

### P2（全部完成 ✅）
_P2 backlog 已清空（「桌面悬浮小组件」已完成，见 Phase 4；「会话级视图」见 Phase 2b；「多设备同步」已实现为 sync export/import 文件合并；「Webhook + MCP Server」见 Phase 4。）_

### P3
_P3 backlog 已清空（`daemon` 子命令 + 自动归档已完成，见 Phase 2b；前端 `fmt` 重复函数收敛亦已完成）。_

## 五、codexU 借鉴进度

| 优先级 | 项 | 状态 |
|---|---|---|
| P0 | Codex JSONL delta + SQLite 回退 + probe + app-server 额度 | ✅ |
| P0 | Codex `reasoning_output_tokens` 端到端（Storage / Proxy / UI 全链路） | ✅ |
| P1 | Claude statusLine 额度 + probe | ✅ |
| P1 | 项目排行、Tool/Skill TOP、Probe 列表 | ✅ |
| P1 | 真实环境 Collector 验证（Claude Code 2025+ 格式） | ✅ |
| P1 | Phase 3 MITM TLS + 自签 CA + 动态叶子证书 | ✅ |
| P1 | 前端 K/M/B 数字格式化 + ReasoningBadge | ✅ |
| P2 | 半年 Token 热力图、托盘额度展示 | ✅ |
| P1 | 今日看板 + 订阅羊毛价值（3 时间窗 API 等效成本 tile） | ✅ |
| P1 | CA 一键安装工具（Windows/macOS/Linux 三平台信任库） | ✅ |
| P1 | MITM chunked/SSE 流式解码 + 流式 reasoning tokens 保真 | ✅ |
| P1 | MITM Content-Encoding（gzip/deflate/br）响应解压 | ✅ |
| P1 | 桌面端 CA 安装/卸载按钮（Settings 走 web `/api/ca/*`） | ✅ |
| P2 | 会话级视图（session grouping） | ✅ |
| P2 | PDF 报表导出（CLI/API/Settings，打印就绪 HTML → 另存为 PDF） | ✅ |
| P2 | 订阅档位配置 → 显式「已省 X%」百分比 | ✅ |
| P2 | Context-window %（每模型上下文窗口占用进度条） | ✅ |
