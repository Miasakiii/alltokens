# AllTokens Frontend

React 19 + Tailwind CSS 4 + Vite 单页应用，嵌入 `alltokens-web`（axum 静态服务）与 Tauri 桌面端。纯 CSS/SVG 图表，无图表库依赖。

## 命令

```bash
npm run dev       # Vite 开发服务器（需后端 :3210 提供 /api）
npm run build     # 产物输出到 dist/（被 web crate 嵌入）
npx tsc -b        # 类型检查（esbuild 不做类型检查，改动后必跑）
```

## 结构

- `src/pages/` — `Dashboard`（单页分析面板）、`Settings`
- `src/components/` — 卡片/图表/表格组件；`src/components/ui/` 共享控件（`SegmentedControl`、`FreshnessLabel`、`primitives.tsx`：`Card` / `Skeleton` / `LoadingRows` / `EmptyState` / `Stat` / `Meter`）
- `src/i18n.tsx` — 中/EN 双语（`LanguageProvider` + `useLang` hook，localStorage key `alltokens-lang` 持久化，默认 `zh`；覆盖全部组件含 Settings、WidgetView、ErrorBoundary）
- `src/hooks/` — 数据获取（`useStats` 系列）、`useWebSocket`（扫描完成推送 + `connected` 状态）、`useTheme`
- `src/api/` — API client 与类型（与 `alltokens-core` model 对齐）
- `src/utils/` — `format.ts`（`formatTokens` / `formatCost` / `formatInt` / `formatPercent`）、`dates.ts`（周期起点 ISO + `formatAge`）
- `public/favicon.svg` — 手写 SVG logo（暖铜色递增柱状图 + token 圆点）

## 设计系统（Warm Ledger）

暖色系低饱和，浅色为默认主题（深色完整保留），由 `src/index.css` 的 CSS 变量驱动双主题：

- 主题 token：`--app-*`（背景/文本/边框/强调色等）、图表色板 `--chart-1` … `--chart-8`
- 组件类：`.surface` / `.surface-2`（卡片容器）、`.btn` / `.icon-btn`（按钮）、`.pill`、`.input`、`.badge-*`、`.skeleton`、`.meter`、`.label-xs`、`.num`
- 组件内禁止硬编码 Tailwind 颜色类（如 `bg-slate-800`），一律使用上述 token 与组件类

## 约定

- 卡片容器统一用 `index.css` 的 `.surface` 类（CSS 变量驱动，深浅主题自动适配），不要硬编码 `bg-slate-800/60 border-slate-700/50 …`
- 文本颜色用 `.text-muted` / `.text-heading`；新增交互控件先沉淀为 `ui/` 共享组件再使用
- 数字列加 `tabular-nums`；token/成本格式化一律走 `utils/format.ts`，不要在组件里写本地 `fmt`
