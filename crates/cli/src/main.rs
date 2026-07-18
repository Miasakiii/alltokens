use alltokens_core::model::{BudgetConfig, Pagination, RequestFilter};
use alltokens_core::pricing::PricingEngine;
use alltokens_core::storage::Storage;
use anyhow::Result;
use chrono::{Local, Utc};
use clap::{Parser, Subcommand};

mod logging;

#[derive(Parser)]
#[command(name = "alltokens", about = "Track AI API token usage & cost")]
#[command(version, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// 数据库路径 (默认 ~/.alltokens/data.db)
    #[arg(long)]
    db: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// 初始化数据库
    Init,

    /// 扫描所有可用工具并采集数据
    Scan,

    /// 今日汇总
    Today,

    /// 查看请求列表
    List {
        /// 按 provider 过滤
        #[arg(long)]
        provider: Option<String>,
        /// 按模型过滤
        #[arg(long)]
        model: Option<String>,
        /// 按工具过滤
        #[arg(long)]
        tool: Option<String>,
        /// 最近 N 天
        #[arg(long)]
        last: Option<String>,
        /// 显示条数
        #[arg(long, default_value = "20")]
        limit: u32,
    },

    /// 统计
    Stats {
        /// 分组维度: provider | model | tool | day
        #[arg(long, default_value = "provider")]
        by: String,
        /// 最近 N 天
        #[arg(long)]
        last: Option<String>,
    },

    /// 成本查看
    Cost {
        /// 货币: usd | cny
        #[arg(long, default_value = "usd")]
        currency: String,
        /// 最近 N 天
        #[arg(long)]
        last: Option<String>,
    },

    /// 启动 Web API 服务
    Serve {
        /// 监听端口
        #[arg(long, default_value = "3210")]
        port: u16,
    },

    /// 后台常驻：定时扫描 + 按 retention_days 自动归档清理
    Daemon {
        /// 扫描间隔（分钟）；省略则读取 general.auto_scan_interval_minutes，仍为 0 时回退 15
        #[arg(long)]
        interval: Option<u64>,
        /// 只执行一个周期后退出（用于 cron / 测试）
        #[arg(long)]
        once: bool,
    },

    /// 启动 MCP Server (stdio)：AI 工具查询用量统计 / 推送 usage 记录
    Mcp,

    /// 多设备同步（文件合并：导出快照 / 合并其他设备的数据库）
    Sync {
        #[command(subcommand)]
        action: SyncAction,
    },

    /// 定价管理
    Pricing {
        #[command(subcommand)]
        action: PricingAction,
    },

    /// 导出 usage 数据 (CSV / JSON / PDF 报表)
    Export {
        /// 导出格式: csv | json | pdf
        #[arg(long, default_value = "csv")]
        format: String,
        /// 输出文件 (默认 stdout)
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
        /// 按 provider 过滤
        #[arg(long)]
        provider: Option<String>,
        /// 按模型过滤
        #[arg(long)]
        model: Option<String>,
        /// 按 collector 过滤
        #[arg(long)]
        collector: Option<String>,
        /// 按工具过滤
        #[arg(long)]
        tool: Option<String>,
        /// 最近 N 天
        #[arg(long)]
        last: Option<String>,
    },

    /// 透明代理 (Phase 3)
    Proxy {
        #[command(subcommand)]
        action: ProxyAction,
    },

    /// CA 证书管理（安装到系统信任库）
    Ca {
        #[command(subcommand)]
        action: CaAction,
    },

    /// 月度预算
    Budget {
        #[command(subcommand)]
        action: BudgetAction,
    },

    /// 探测采集器数据源 (不写入数据库)
    Probe {
        /// 采集器 ID (省略则列出全部状态): codex | claude | cursor | opencode | windsurf | ...
        collector: Option<String>,
        /// 输出 JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ProxyAction {
    /// 启动 HTTP 转发代理
    Start {
        /// 监听地址
        #[arg(long, default_value = "127.0.0.1:7890")]
        listen: String,
        /// 启用 MITM TLS 拦截（自动生成 CA 证书，解密 HTTPS 流量）
        #[arg(long)]
        mitm: bool,
        /// CA 证书目录路径（默认 ~/.alltokens/ca/）
        #[arg(long)]
        ca_dir: Option<String>,
    },
    /// 显示代理状态说明
    Status,
}

#[derive(Subcommand)]
enum CaAction {
    /// 安装 CA 证书到系统信任库
    Install {
        /// CA 证书目录路径（默认 ~/.alltokens/ca/）
        #[arg(long)]
        ca_dir: Option<String>,
    },
    /// 从系统信任库移除 CA 证书
    Uninstall {
        /// CA 证书目录路径（默认 ~/.alltokens/ca/）
        #[arg(long)]
        ca_dir: Option<String>,
    },
    /// 查询 CA 证书是否已安装
    Status {
        /// CA 证书目录路径（默认 ~/.alltokens/ca/）
        #[arg(long)]
        ca_dir: Option<String>,
    },
    /// 显示 CA 证书文件路径
    Path {
        /// CA 证书目录路径（默认 ~/.alltokens/ca/）
        #[arg(long)]
        ca_dir: Option<String>,
    },
}

#[derive(Subcommand)]
enum PricingAction {
    /// 列出所有定价
    List,
}

#[derive(Subcommand)]
enum SyncAction {
    /// 导出本地数据库为一致快照文件（可放入共享目录/U盘）
    Export {
        /// 输出文件路径
        #[arg(short, long)]
        output: std::path::PathBuf,
    },
    /// 从另一个 AllTokens 数据库文件合并用量记录（自动去重、幂等）
    Import {
        /// 源数据库文件路径（其他设备 sync export 的快照或其 data.db）
        file: std::path::PathBuf,
    },
}

#[derive(Subcommand)]
enum BudgetAction {
    /// 设置月预算 (USD)
    Set {
        /// 月预算上限 (USD)
        #[arg(long)]
        monthly: f64,
        /// 禁用预算告警
        #[arg(long)]
        disable: bool,
    },
    /// 查看预算使用情况
    Status,
}

fn get_db_path(cli_db: &Option<String>) -> std::path::PathBuf {
    if let Some(path) = cli_db {
        return std::path::PathBuf::from(path);
    }

    let data_dir = dirs::data_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("share")))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let alltokens_dir = data_dir.join("alltokens");
    std::fs::create_dir_all(&alltokens_dir).ok();
    alltokens_dir.join("data.db")
}

fn parse_date_range(last: &Option<String>) -> Option<String> {
    last.as_ref().map(|s| {
        let days: i64 = s.trim_end_matches('d').parse().unwrap_or(7);
        let date = Utc::now() - chrono::Duration::days(days);
        date.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_path = get_db_path(&cli.db);

    // Long-running subcommands additionally log to a persistent file.
    let log_file = match &cli.command {
        Commands::Serve { .. } | Commands::Daemon { .. } => {
            Some(logging::default_log_path(&db_path))
        }
        _ => None,
    };
    // MCP 的 stdout 是 JSON-RPC 协议通道，日志只能走 stderr。
    if matches!(cli.command, Commands::Mcp) {
        logging::init_stderr()?;
    } else {
        logging::init(log_file)?;
    }

    match cli.command {
        Commands::Init => {
            let db_dir = db_path.parent().unwrap();
            std::fs::create_dir_all(db_dir)?;
            let _storage = Storage::open(&db_path)?;
            println!("✅ Initialized database at {}", db_path.display());
        }

        Commands::Scan => {
            let storage = Storage::open(&db_path)?;
            let pricing = storage.load_pricing_engine()?;
            let result = alltokens_web::run_scan(&storage, &pricing).await?;

            println!("\n📊 Total: {} new records inserted", result.total);
            alltokens_web::notify_running_servers(result.total).await;
        }

        Commands::Today => {
            let storage = Storage::open(&db_path)?;
            let today = Local::now().format("%Y-%m-%d").to_string();
            let filter = RequestFilter {
                start_date: Some(format!("{today}T00:00:00+08:00")),
                ..Default::default()
            };

            let overview = storage.get_overview(&filter)?;

            println!("📊 Today's Usage");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("  Requests:       {}", overview.total_requests);
            println!("  Input Tokens:   {}", format_tokens(overview.total_input_tokens));
            println!("  Output Tokens:  {}", format_tokens(overview.total_output_tokens));
            println!("  Cache Read:     {}", format_tokens(overview.total_cache_read_tokens));
            println!("  Cache Creation: {}", format_tokens(overview.total_cache_creation_tokens));
            println!("  Total Tokens:   {}", format_tokens(overview.total_tokens));
            println!("  Cache Hit Rate: {:.1}%", overview.cache_hit_rate * 100.0);
            println!("  Cost (USD):     ${:.4}", overview.total_cost_usd);
            println!("  Cost (CNY):     ¥{:.2}", overview.total_cost_cny);
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        }

        Commands::List {
            provider,
            model,
            tool,
            last,
            limit,
        } => {
            let storage = Storage::open(&db_path)?;
            let filter = RequestFilter {
                provider,
                model,
                tool,
                start_date: parse_date_range(&last),
                ..Default::default()
            };
            let pagination = Pagination {
                page: 0,
                page_size: limit,
            };

            let result = storage.get_requests(&filter, &pagination)?;

            println!(
                "📋 Requests (showing {} of {})\n",
                result.items.len(),
                result.total
            );
            println!(
                "{:<20} {:<15} {:<30} {:>10} {:>10} {:>10} {:>10}",
                "Time", "Provider", "Model", "Input", "Output", "Cache", "Cost"
            );
            println!("{}", "─".repeat(110));

            for r in &result.items {
                let time = r.timestamp.format("%Y-%m-%d %H:%M").to_string();
                let model_display = if r.model.len() > 28 {
                    format!("{}…", &r.model[..27])
                } else {
                    r.model.clone()
                };
                println!(
                    "{:<20} {:<15} {:<30} {:>10} {:>10} {:>10} {:>10}",
                    time,
                    r.provider,
                    model_display,
                    format_tokens(r.input_tokens),
                    format_tokens(r.output_tokens),
                    format_tokens(r.cache_read_tokens),
                    format!("${:.4}", r.cost_usd),
                );
            }
        }

        Commands::Stats { by, last } => {
            let storage = Storage::open(&db_path)?;
            let filter = RequestFilter {
                start_date: parse_date_range(&last),
                ..Default::default()
            };

            match by.as_str() {
                "provider" => {
                    let stats = storage.get_provider_stats(&filter)?;
                    println!("📊 Stats by Provider\n");
                    println!(
                        "{:<15} {:>10} {:>15} {:>12} {:>10}",
                        "Provider", "Requests", "Tokens", "Cost (USD)", "Cache Hit"
                    );
                    println!("{}", "─".repeat(65));
                    for s in &stats {
                        println!(
                            "{:<15} {:>10} {:>15} {:>12} {:>9.1}%",
                            s.provider,
                            s.request_count,
                            format_tokens(s.total_tokens),
                            format!("${:.4}", s.total_cost_usd),
                            s.cache_hit_rate * 100.0,
                        );
                    }
                }
                "model" => {
                    let stats = storage.get_model_stats(&filter)?;
                    println!("📊 Stats by Model\n");
                    println!(
                        "{:<15} {:<25} {:>10} {:>15} {:>12} {:>10}",
                        "Provider", "Model", "Requests", "Tokens", "Cost (USD)", "Cache Hit"
                    );
                    println!("{}", "─".repeat(90));
                    for s in &stats {
                        let model_display = if s.model.len() > 23 {
                            format!("{}…", &s.model[..22])
                        } else {
                            s.model.clone()
                        };
                        println!(
                            "{:<15} {:<25} {:>10} {:>15} {:>12} {:>9.1}%",
                            s.provider,
                            model_display,
                            s.request_count,
                            format_tokens(s.total_tokens),
                            format!("${:.4}", s.total_cost_usd),
                            s.cache_hit_rate * 100.0,
                        );
                    }
                }
                "tool" => {
                    let stats = storage.get_tool_stats(&filter)?;
                    println!("📊 Stats by Tool\n");
                    println!(
                        "{:<15} {:<15} {:>10} {:>15} {:>12}",
                        "Collector", "Tool", "Requests", "Tokens", "Cost (USD)"
                    );
                    println!("{}", "─".repeat(70));
                    for s in &stats {
                        println!(
                            "{:<15} {:<15} {:>10} {:>15} {:>12}",
                            s.collector,
                            s.tool.as_deref().unwrap_or("-"),
                            s.request_count,
                            format_tokens(s.total_tokens),
                            format!("${:.4}", s.total_cost_usd),
                        );
                    }
                }
                "day" => {
                    let stats = storage.get_daily_trends(&filter)?;
                    println!("📊 Daily Trends\n");
                    println!(
                        "{:<12} {:>10} {:>15} {:>15} {:>12} {:>10}",
                        "Date", "Requests", "Input", "Output", "Cost (USD)", "Cache Hit"
                    );
                    println!("{}", "─".repeat(78));
                    let mut daily: std::collections::HashMap<
                        String,
                        (u64, u64, u64, f64, u64, u64),
                    > = std::collections::HashMap::new();
                    for s in &stats {
                        let entry = daily.entry(s.date.clone()).or_insert((0, 0, 0, 0.0, 0, 0));
                        entry.0 += s.request_count;
                        entry.1 += s.total_input;
                        entry.2 += s.total_output;
                        entry.3 += s.total_cost_usd;
                        entry.4 += s.total_cache_read;
                        entry.5 += s.total_cache_creation;
                    }
                    let mut dates: Vec<_> = daily.into_iter().collect();
                    dates.sort_by(|a, b| b.0.cmp(&a.0));
                    for (date, (count, input, output, cost, cache_read, cache_creation)) in &dates {
                        let cacheable = *input + *cache_creation + *cache_read;
                        let hit_rate = if cacheable > 0 {
                            *cache_read as f64 / cacheable as f64 * 100.0
                        } else {
                            0.0
                        };
                        println!(
                            "{:<12} {:>10} {:>15} {:>15} {:>12} {:>9.1}%",
                            date,
                            count,
                            format_tokens(*input),
                            format_tokens(*output),
                            format!("${:.4}", cost),
                            hit_rate,
                        );
                    }
                }
                _ => {
                    println!("Unknown grouping: {by}. Use: provider, model, tool, day");
                }
            }
        }

        Commands::Cost { currency, last } => {
            let storage = Storage::open(&db_path)?;
            let filter = RequestFilter {
                start_date: parse_date_range(&last),
                ..Default::default()
            };
            let overview = storage.get_overview(&filter)?;

            match currency.as_str() {
                "cny" => {
                    println!("💰 Total Cost: ¥{:.2}", overview.total_cost_cny);
                }
                _ => {
                    println!("💰 Total Cost: ${:.4}", overview.total_cost_usd);
                }
            }
        }

        Commands::Pricing { action } => match action {
            PricingAction::List => {
                let pricing = PricingEngine::from_builtin();
                let mut entries: Vec<_> = pricing.all_entries();
                entries.sort_by(|a, b| a.provider.cmp(&b.provider).then(a.model.cmp(&b.model)));

                println!("📋 Pricing Table\n");
                println!(
                    "{:<15} {:<30} {:>10} {:>10} {:>10} {:>10}",
                    "Provider", "Model", "Input", "Output", "Cache R", "Cache W"
                );
                println!("{}", "─".repeat(90));
                for e in &entries {
                    println!(
                        "{:<15} {:<30} {:>10} {:>10} {:>10} {:>10}",
                        e.provider,
                        e.model,
                        format!("${}", e.input_per_mtok),
                        format!("${}", e.output_per_mtok),
                        format!("${}", e.cache_read_per_mtok),
                        format!("${}", e.cache_create_per_mtok),
                    );
                }
            }
        },

        Commands::Serve { port } => {
            let storage = Storage::open(&db_path)?;
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
            // 查找前端静态文件
            let static_dir = std::env::current_dir().ok()
                .map(|d| d.join("frontend/dist"))
                .filter(|d| d.exists());
            println!("🌐 Starting AllTokens on http://{}", addr);
            if static_dir.is_some() {
                println!("   Dashboard: http://{}", addr);
            } else {
                println!("   API only:  http://{}/api/overview", addr);
                println!("   (run 'npm run build' in frontend/ for dashboard)");
            }
            let mut config = alltokens_web::WebConfig::new(addr, storage);
            if let Some(d) = static_dir {
                config = config.with_static(d);
            }
            alltokens_web::start_web(config).await?;
        },

        Commands::Daemon { interval, once } => {
            let storage = Storage::open(&db_path)?;
            let pricing = storage.load_pricing_engine()?;
            // Resolve interval: CLI flag > general config > 15-minute fallback.
            let minutes = interval.unwrap_or_else(|| {
                let configured = storage
                    .get_general_config()
                    .map(|c| c.auto_scan_interval_minutes as u64)
                    .unwrap_or(0);
                if configured > 0 { configured } else { 15 }
            });
            let period = std::time::Duration::from_secs(minutes.max(1) * 60);
            println!("\u{1f6f0}\u{fe0f}  AllTokens daemon started (every {minutes}m). Ctrl+C to stop.");
            loop {
                let now = Local::now().format("%Y-%m-%d %H:%M:%S");
                println!("\n\u{23f1}\u{fe0f}  Cycle @ {now}");
                match alltokens_web::run_maintenance_cycle(&storage, &pricing).await {
                    Ok(result) => {
                        println!(
                            "   \u{1f4ca} {} new records \u{2022} \u{1f9f9} {} purged",
                            result.scan.total, result.purged
                        );
                        alltokens_web::notify_running_servers(result.scan.total).await;
                    }
                    Err(e) => println!("   \u{274c} Cycle error: {e}"),
                }
                if once {
                    break;
                }
                tokio::select! {
                    _ = tokio::time::sleep(period) => {}
                    _ = tokio::signal::ctrl_c() => {
                        println!("\n\u{1f44b} Daemon stopped");
                        break;
                    }
                }
            }
        },

        Commands::Mcp => {
            let storage = Storage::open(&db_path)?;
            let server = alltokens_collectors::mcp::McpServer::new(storage)?;
            alltokens_collectors::mcp::run_stdio(server).await?;
        }

        Commands::Sync { action } => match action {
            SyncAction::Export { output } => {
                let storage = Storage::open(&db_path)?;
                // WAL checkpoint 截断后，主 db 文件即自包含一致快照。
                storage.checkpoint()?;
                std::fs::copy(&db_path, &output)?;
                let count = Storage::open(&output)?.count()?;
                println!(
                    "✅ Exported snapshot ({count} records) to {}",
                    output.display()
                );
                println!("   另一台设备上运行: alltokens sync import {}", output.display());
            }
            SyncAction::Import { file } => {
                let storage = Storage::open(&db_path)?;
                let result = storage.merge_from(&file)?;
                println!("🔄 Merge complete from {}", file.display());
                println!(
                    "   scanned {} · inserted {} · skipped {} (duplicates)",
                    result.scanned, result.inserted, result.skipped
                );
            }
        },

        Commands::Export {
            format,
            output,
            provider,
            model,
            collector,
            tool,
            last,
        } => {
            let storage = Storage::open(&db_path)?;
            let filter = RequestFilter {
                provider,
                model,
                collector,
                tool,
                start_date: parse_date_range(&last),
                ..Default::default()
            };
            let records = storage.export_requests(&filter)?;

            let body = match format.as_str() {
                "csv" => alltokens_core::export::to_csv(&records),
                "json" => alltokens_core::export::to_json(&records)?,
                "pdf" | "html" => alltokens_core::export::to_html_report(&records),
                _ => {
                    anyhow::bail!("Unknown format: {format}. Use: csv, json, pdf");
                }
            };

            if let Some(path) = output {
                std::fs::write(&path, &body)?;
                println!(
                    "✅ Exported {} records to {}",
                    records.len(),
                    path.display()
                );
            } else {
                print!("{body}");
            }
        },

        Commands::Budget { action } => match action {
            BudgetAction::Set { monthly, disable } => {
                let storage = Storage::open(&db_path)?;
                let config = BudgetConfig {
                    monthly_usd: Some(monthly),
                    enabled: !disable,
                };
                storage.set_budget_config(&config)?;
                if disable {
                    println!("✅ Monthly budget set to ${monthly:.2} (alerts disabled)");
                } else {
                    println!("✅ Monthly budget set to ${monthly:.2} (alerts enabled)");
                }
            }
            BudgetAction::Status => {
                let storage = Storage::open(&db_path)?;
                let config = storage.get_budget_config()?;
                let today = Local::now();
                let month_start = format!("{}-01T00:00:00+08:00", today.format("%Y-%m"));
                let filter = RequestFilter {
                    start_date: Some(month_start),
                    ..Default::default()
                };
                let overview = storage.get_overview(&filter)?;

                println!("💰 Budget Status");
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                match (config.enabled, config.monthly_usd) {
                    (false, Some(limit)) => {
                        println!("  Alerts:         disabled");
                        println!("  Monthly limit:  ${limit:.2} (not enforced)");
                    }
                    (false, None) => {
                        println!("  Alerts:         disabled");
                        println!("  Monthly limit:  not set");
                    }
                    (true, None) => {
                        println!("  Alerts:         enabled");
                        println!("  Monthly limit:  not set");
                        println!("  Used this month: ${:.2}", overview.total_cost_usd);
                    }
                    (true, Some(limit)) => {
                        let used = overview.total_cost_usd;
                        let pct = if limit > 0.0 { used / limit * 100.0 } else { 0.0 };
                        println!("  Monthly limit:  ${limit:.2}");
                        println!("  Used this month: ${used:.2} ({pct:.1}%)");
                        if used >= limit {
                            println!("  Status:         ⚠️  OVER BUDGET");
                        } else if pct >= 80.0 {
                            println!("  Status:         ⚠️  Approaching limit (≥80%)");
                            println!("  Remaining:      ${:.2}", limit - used);
                        } else {
                            println!("  Remaining:      ${:.2}", limit - used);
                        }
                    }
                }
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            }
        },

        Commands::Proxy { action } => match action {
            ProxyAction::Start { listen, mitm, ca_dir } => {
                let addr: std::net::SocketAddr = listen.parse()
                    .map_err(|_| anyhow::anyhow!("Invalid listen address: {listen}"))?;
                let storage = Storage::open(&db_path)?;
                let pricing = PricingEngine::from_builtin();

                let config = alltokens_proxy::ProxyConfig {
                    listen_addr: addr,
                    ca_cert_path: ca_dir.map(std::path::PathBuf::from),
                    mitm_enabled: mitm,
                };

                if mitm {
                    let ca_path = config.ca_dir();
                    println!("🔒 Starting MITM proxy on {addr}");
                    println!("   CA directory: {}", ca_path.display());
                    println!("   HTTPS interception: enabled for known API hosts");
                    println!("   Install CA cert from: {}", ca_path.join("alltokens-ca.crt").display());
                } else {
                    println!("🔌 Starting forward proxy on {addr}");
                    println!("   Tip: use --mitm to enable HTTPS interception");
                }
                println!("   Intercepted usage → SQLite ({})", db_path.display());
                println!("   Press Ctrl+C to stop");

                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        println!("\nStopping proxy...");
                    }
                    result = alltokens_proxy::start_proxy_with_storage(config, storage, pricing) => {
                        result?;
                    }
                }
            }
            ProxyAction::Status => {
                println!("Proxy status:");
                println!("  - HTTP relay + CONNECT tunnel: ✅ implemented");
                println!("  - Usage extraction (OpenAI/Anthropic/Qwen/etc): ✅ implemented");
                println!("  - MITM TLS / dynamic cert: ✅ implemented (use --mitm flag)");
                println!("  - CA certificate generation: ✅ auto-generated on first use");
                println!("  - CA trust-store install: run `alltokens ca install`");
            }
        },

        Commands::Ca { action } => {
            // 解析 CA 目录（复用 ProxyConfig 默认路径逻辑）
            let resolve_dir = |ca_dir: Option<String>| {
                alltokens_proxy::ProxyConfig {
                    ca_cert_path: ca_dir.map(std::path::PathBuf::from),
                    ..Default::default()
                }
                .ca_dir()
            };
            match action {
                CaAction::Install { ca_dir } => {
                    let dir = resolve_dir(ca_dir);
                    // 确保证书已生成
                    alltokens_proxy::CertificateAuthority::load_or_generate(&dir)?;
                    let cert_path =
                        alltokens_proxy::CertificateAuthority::cert_path(&dir);
                    println!("🔐 Installing CA into system trust store...");
                    println!("   Cert: {}", cert_path.display());
                    alltokens_proxy::install(&cert_path)?;
                    println!("✅ CA installed. HTTPS interception (--mitm) is now trusted.");
                    match alltokens_proxy::TrustStore::detect() {
                        alltokens_proxy::TrustStore::MacOs => {
                            println!("   (macOS 首次可能需要在钥匙串中确认信任)");
                        },
                        alltokens_proxy::TrustStore::Linux => {
                            println!("   (Firefox/Chromium 使用独立 NSS 信任库，可能需单独导入)");
                        },
                        alltokens_proxy::TrustStore::Windows => {},
                    }
                },
                CaAction::Uninstall { ca_dir } => {
                    let dir = resolve_dir(ca_dir);
                    let cert_path =
                        alltokens_proxy::CertificateAuthority::cert_path(&dir);
                    println!("🧹 Removing CA from system trust store...");
                    alltokens_proxy::uninstall(&cert_path)?;
                    println!("✅ CA removed from trust store.");
                },
                CaAction::Status { ca_dir } => {
                    let dir = resolve_dir(ca_dir);
                    let cert_path =
                        alltokens_proxy::CertificateAuthority::cert_path(&dir);
                    let exists = cert_path.exists();
                    println!("CA file: {} ({})", cert_path.display(),
                        if exists { "present" } else { "not generated" });
                    match alltokens_proxy::status(&cert_path)? {
                        alltokens_proxy::CaInstallStatus::Installed => {
                            println!("Trust store: ✅ installed");
                        },
                        alltokens_proxy::CaInstallStatus::NotInstalled => {
                            println!("Trust store: ❌ not installed (run `alltokens ca install`)");
                        },
                        alltokens_proxy::CaInstallStatus::Unknown => {
                            println!("Trust store: ⚠️ unknown (could not query platform tool)");
                        },
                    }
                },
                CaAction::Path { ca_dir } => {
                    let dir = resolve_dir(ca_dir);
                    let cert_path =
                        alltokens_proxy::CertificateAuthority::cert_path(&dir);
                    println!("{}", cert_path.display());
                },
            }
        },

        Commands::Probe { collector, json } => {
            match collector {
                None => {
                    let statuses = alltokens_collectors::probe::list_collector_probe_status();
                    if json {
                        println!("{}", serde_json::to_string_pretty(&statuses)?);
                    } else {
                        print_probe_status_list(&statuses);
                    }
                }
                Some(ref name) => {
                    let id = alltokens_collectors::probe::normalize_probe_collector_id(name);
                    match id {
                        "codex" => {
                            let c = alltokens_collectors::codex::CodexCollector::new();
                            let probe = c.probe()?;
                            if json {
                                println!("{}", serde_json::to_string_pretty(&probe)?);
                            } else {
                                print_codex_probe(&probe);
                            }
                        }
                        "claude_code" => {
                            let c = alltokens_collectors::claude_code::ClaudeCodeCollector::new();
                            let probe = c.probe()?;
                            if json {
                                println!("{}", serde_json::to_string_pretty(&probe)?);
                            } else {
                                print_claude_probe(&probe);
                            }
                        }
                        "cursor" => {
                            let c = alltokens_collectors::cursor::CursorCollector::new();
                            let probe = c.probe()?;
                            if json {
                                println!("{}", serde_json::to_string_pretty(&probe)?);
                            } else {
                                print_basic_probe(&probe);
                            }
                        }
                        "opencode" => {
                            let c = alltokens_collectors::opencode::OpenCodeCollector::new();
                            let probe = c.probe()?;
                            if json {
                                println!("{}", serde_json::to_string_pretty(&probe)?);
                            } else {
                                print_basic_probe(&probe);
                            }
                        }
                        "windsurf" => {
                            let c = alltokens_collectors::windsurf::WindsurfCollector::new();
                            let probe = c.probe()?;
                            if json {
                                println!("{}", serde_json::to_string_pretty(&probe)?);
                            } else {
                                print_basic_probe(&probe);
                            }
                        }
                        other => {
                            let supported = alltokens_collectors::probe::probe_supported_ids().join(", ");
                            anyhow::bail!(
                                "Unknown collector for probe: {other}. Supported: {supported} (also: claude)"
                            );
                        }
                    }
                }
            }
        },
    }

    Ok(())
}

fn print_probe_status_list(statuses: &[alltokens_collectors::probe::CollectorProbeStatus]) {
    println!("🔍 Collector probe status");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  {:<18} {:<22} {:>10}  probe", "ID", "Name", "Detected");
    println!("  {}", "─".repeat(58));
    for row in statuses {
        let detected = if row.detected { "yes" } else { "no" };
        let probe = if row.probe_supported { "yes" } else { "—" };
        println!(
            "  {:<18} {:<22} {:>10}  {probe}",
            row.id, row.name, detected
        );
    }
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  Run `alltokens probe <id>` for details (codex, claude, cursor, opencode, windsurf)");
}

fn print_basic_probe(probe: &alltokens_collectors::probe::BasicProbeResult) {
    println!("🔍 {} probe", probe.collector_name);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(
        "  Detected:           {}",
        if probe.detected { "yes" } else { "no" }
    );
    println!("  Data paths:         {}", probe.data_paths.len());
    for path in &probe.data_paths {
        println!("    - {path}");
    }
    println!("  Data files:         {}", probe.data_files);
    println!("  Sample records:     {}", probe.sample_records);
    if !probe.errors.is_empty() {
        println!("  Errors:");
        for err in &probe.errors {
            println!("    - {err}");
        }
    }
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
}

fn print_codex_probe(probe: &alltokens_collectors::codex::CodexProbeResult) {
    println!("🔍 Codex probe");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  Roots:              {}", probe.codex_roots.len());
    for root in &probe.codex_roots {
        println!("    - {root}");
    }
    println!("  JSONL files:        {}", probe.jsonl_files);
    println!("  Session JSON files: {}", probe.session_json_files);
    println!("  SQLite DBs:         {}", probe.sqlite_paths.len());
    for db in &probe.sqlite_paths {
        println!("    - {db}");
    }
    println!("  Detailed records:   {}", probe.detailed_records);
    println!("  Coarse records:     {}", probe.coarse_records);
    println!("  Sessions (detailed): {}", probe.sessions_with_detailed);
    if let Some(ref quota) = probe.quota {
        println!();
        println!("  Quota (app-server):");
        if let Some(plan) = &quota.plan_type {
            println!("    Plan:       {plan}");
        }
        if let Some(w) = &quota.five_hour {
            print_window("5h", w);
        } else {
            println!("    5h window:  --");
        }
        if let Some(w) = &quota.seven_day {
            print_window("7d", w);
        } else {
            println!("    7d window:  --");
        }
    } else if let Some(ref err) = probe.quota_error {
        println!();
        println!("  Quota: unavailable ({err})");
    }
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
}

fn print_claude_probe(probe: &alltokens_collectors::claude_code::ClaudeProbeResult) {
    println!("🔍 Claude Code probe");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  Data dirs:          {}", probe.data_dirs.len());
    for dir in &probe.data_dirs {
        println!("    - {dir}");
    }
    println!("  Usage files:        {}", probe.usage_files);
    println!("  Snapshot files:     {}", probe.snapshot_paths.len());
    for path in &probe.snapshot_paths {
        println!("    - {path}");
    }
    if let Some(ref quota) = probe.quota {
        println!();
        println!("  Quota (statusLine snapshot):");
        if quota.is_stale {
            println!("    Status:     stale (>15m)");
        }
        if let Some(w) = &quota.five_hour {
            print_window("5h", w);
        } else {
            println!("    5h window:  --");
        }
        if let Some(w) = &quota.seven_day {
            print_window("7d", w);
        } else {
            println!("    7d window:  --");
        }
    } else if let Some(ref err) = probe.quota_error {
        println!();
        println!("  Quota: unavailable ({err})");
    }
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
}

fn print_window(label: &str, window: &alltokens_core::model::CodexQuotaWindow) {
    let remaining = window
        .remaining_percent
        .map(|p| format!("{p}% remaining"))
        .unwrap_or_else(|| "--".to_string());
    let used = window
        .used_percent
        .map(|p| format!("{p}% used"))
        .unwrap_or_else(|| "--".to_string());
    println!("    {label} window: {remaining} ({used})");
}

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
