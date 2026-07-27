# AGENTS.md — Agent 导航入口

面向 coding agent 的最小路由页。详细内容一律以 [README.md](README.md) 为准，本文件只负责指路。

## 项目一句话

AllTokens：本地优先的 AI token 用量与成本追踪（Rust workspace + React 前端 + Tauri 桌面端）。

## 目录路由

完整结构见 README「项目结构」一节。核心位置：

| 位置 | 职责 |
|---|---|
| `crates/core/` | 数据模型、SQLite 存储、定价引擎、导出 |
| `crates/collectors/` | 23 个数据采集器 + MCP Server（`mcp.rs`）；fixtures 测试在 `tests/` |
| `crates/web/` | axum Web API + WebSocket（端点清单见 README「Web API」） |
| `crates/proxy/` | 转发代理 + MITM TLS 解密 + CA 管理 |
| `crates/cli/` | CLI 入口（命令清单见 README「CLI 命令」） |
| `frontend/` | React 19 + Tailwind 4 Dashboard（构建产物嵌入 web crate） |
| `src-tauri/` | Tauri 2.x 桌面端（托盘、小组件、通知） |
| `pricing/builtin.toml` | 内置模型定价表 |

## 风险区域（改动前先读 README 对应说明）

- **CA 信任库写入**：`crates/proxy/src/ca_install.rs`、CLI `alltokens ca install`、API `POST /api/ca/install`。属敏感操作——CLI 默认交互确认，API 需显式 `{"confirm": true}` 否则仅 dry-run。不要削弱这些确认门槛。
- **MITM 解密**：`crates/proxy/src/mitm.rs` / `intercept.rs`，涉及自签 CA 动态签发。
- **存储 schema**：`crates/core/src/schema.rs` / `storage.rs`，改动需考虑既有用户数据库的迁移与 `sync import` 幂等去重。
- **Webhook ingest**：`POST /api/ingest` 有 ≤1000 条批量上限与字段校验，勿放宽。

## 本地开发循环

```bash
# 构建 + 全量验证（与 CI 一致，CI 见 .github/workflows/ci.yml）
cargo clippy --workspace --all-targets
cargo test --workspace                        # Rust 测试
cd frontend && npm ci && npx tsc -b && npx --yes oxlint@1.75.0   # 前端 typecheck + lint
cd frontend && npm run build                  # 前端构建（web crate 嵌入 dist/）

# 运行
cargo build --release -p alltokens-cli
./target/release/alltokens init && ./target/release/alltokens scan
./target/release/alltokens serve --port 3210  # Dashboard: http://127.0.0.1:3210

# 桌面端
cd src-tauri && npx @tauri-apps/cli dev
```

Windows 下二进制路径为 `.\target\release\alltokens.exe`。

## 约定

- `PLAN.md` / `STATUS.md` / `SHARED_TASK_NOTES.md` 是**私有规划笔记**，已被 `.gitignore` 排除，**不要提交入库**（本地存在属正常）。
- 分支保护要求 CI（Rust clippy+test、Frontend typecheck+lint）通过后才能合入 `main`。
- 遵循最小改动原则；新增 Collector 参考 `crates/collectors/src/` 现有实现与 `tests/fixtures/` 模式。
