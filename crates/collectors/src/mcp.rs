//! MCP (Model Context Protocol) server — Phase 4 Layer 3.
//!
//! Speaks newline-delimited JSON-RPC 2.0 over stdio so AI tools can both
//! query usage statistics and push usage records (`report_usage`) into the
//! local database. Hand-rolled minimal protocol implementation — zero new
//! dependencies.
//!
//! stdout carries protocol frames only; logs must go to stderr (the CLI
//! wires tracing accordingly for the `mcp` subcommand).

use alltokens_core::model::{Pagination, RequestFilter, UsageRecord};
use alltokens_core::pricing::PricingEngine;
use alltokens_core::storage::Storage;
use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::{json, Value};

/// MCP protocol revision implemented by this server.
const PROTOCOL_VERSION: &str = "2024-11-05";

/// MCP server bound to a local database.
pub struct McpServer {
    storage: Storage,
    pricing: PricingEngine,
}

impl McpServer {
    pub fn new(storage: Storage) -> Result<Self> {
        let pricing = storage.load_pricing_engine()?;
        Ok(Self { storage, pricing })
    }

    /// Handle one newline-delimited JSON-RPC message. Returns the response
    /// line, or `None` for notifications (messages without a non-null `id`).
    pub fn handle_message(&self, line: &str) -> Option<String> {
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                return Some(
                    json!({
                        "jsonrpc": "2.0",
                        "id": Value::Null,
                        "error": { "code": -32700, "message": "Parse error" }
                    })
                    .to_string(),
                );
            }
        };
        let id = match msg.get("id") {
            Some(v) if !v.is_null() => v.clone(),
            _ => return None, // notification — no response
        };
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(Value::Null);

        let result = match method {
            "initialize" => Ok(json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "alltokens",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(tools_list()),
            "tools/call" => self.call_tool(&params),
            _ => Err((-32601, format!("Method not found: {method}"))),
        };

        Some(match result {
            Ok(payload) => json!({ "jsonrpc": "2.0", "id": id, "result": payload }).to_string(),
            Err((code, message)) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": code, "message": message }
            })
            .to_string(),
        })
    }

    fn call_tool(&self, params: &Value) -> std::result::Result<Value, (i64, String)> {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let outcome = match name {
            "get_overview" => self.tool_get_overview(&args),
            "get_stats" => self.tool_get_stats(&args),
            "list_requests" => self.tool_list_requests(&args),
            "get_budget_status" => self.tool_get_budget_status(),
            "report_usage" => self.tool_report_usage(&args),
            _ => return Err((-32602, format!("Unknown tool: {name}"))),
        };
        match outcome {
            Ok(value) => Ok(json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&value).unwrap_or_default(),
                }]
            })),
            // 工具级失败按 MCP 约定走 isError 内容，而非 JSON-RPC 协议错误。
            Err(e) => Ok(json!({
                "content": [{ "type": "text", "text": format!("Error: {e:#}") }],
                "isError": true,
            })),
        }
    }

    fn tool_get_overview(&self, args: &Value) -> Result<Value> {
        let filter = RequestFilter {
            start_date: parse_last(args),
            ..Default::default()
        };
        let stats = self.storage.get_overview(&filter)?;
        Ok(serde_json::to_value(stats)?)
    }

    fn tool_get_stats(&self, args: &Value) -> Result<Value> {
        let by = args.get("by").and_then(Value::as_str).unwrap_or("provider");
        let filter = RequestFilter {
            start_date: parse_last(args),
            ..Default::default()
        };
        let value = match by {
            "provider" => serde_json::to_value(self.storage.get_provider_stats(&filter)?)?,
            "model" => serde_json::to_value(self.storage.get_model_stats(&filter)?)?,
            "tool" => serde_json::to_value(self.storage.get_tool_stats(&filter)?)?,
            _ => return Err(anyhow!("Invalid 'by': {by} (expected provider|model|tool)")),
        };
        Ok(value)
    }

    fn tool_list_requests(&self, args: &Value) -> Result<Value> {
        let filter = RequestFilter {
            provider: arg_string(args, "provider"),
            model: arg_string(args, "model"),
            tool: arg_string(args, "tool"),
            start_date: parse_last(args),
            ..Default::default()
        };
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(20)
            .clamp(1, 100) as u32;
        let page = self
            .storage
            .get_requests(&filter, &Pagination { page: 0, page_size: limit })?;
        Ok(serde_json::to_value(page)?)
    }

    fn tool_get_budget_status(&self) -> Result<Value> {
        let config = self.storage.get_budget_config()?;
        let now = chrono::Local::now();
        let month_start = format!("{}-01T00:00:00{}", now.format("%Y-%m"), now.format("%:z"));
        let filter = RequestFilter {
            start_date: Some(month_start),
            ..Default::default()
        };
        let overview = self.storage.get_overview(&filter)?;
        let used_percent = config.monthly_usd.map(|m| {
            if m > 0.0 {
                overview.total_cost_usd / m * 100.0
            } else {
                0.0
            }
        });
        Ok(json!({
            "enabled": config.enabled,
            "monthly_usd": config.monthly_usd,
            "month_cost_usd": overview.total_cost_usd,
            "month_cost_cny": overview.total_cost_cny,
            "used_percent": used_percent,
        }))
    }

    fn tool_report_usage(&self, args: &Value) -> Result<Value> {
        let required = |key: &str| -> Result<String> {
            args.get(key)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .ok_or_else(|| anyhow!("Missing required field: {key}"))
        };
        let provider = required("provider")?;
        let model = required("model")?;
        let timestamp = args
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        let mut record = UsageRecord {
            id: None,
            timestamp,
            collector: "mcp".to_string(),
            tool: arg_string(args, "tool"),
            provider,
            model,
            input_tokens: arg_u64(args, "input_tokens"),
            output_tokens: arg_u64(args, "output_tokens"),
            reasoning_tokens: arg_u64(args, "reasoning_tokens"),
            cache_read_tokens: arg_u64(args, "cache_read_tokens"),
            cache_creation_tokens: arg_u64(args, "cache_creation_tokens"),
            total_tokens: arg_u64(args, "total_tokens"),
            cost_usd: args.get("cost_usd").and_then(Value::as_f64).unwrap_or(0.0),
            cost_cny: 0.0,
            latency_ms: args.get("latency_ms").and_then(Value::as_u64),
            is_stream: args.get("is_stream").and_then(Value::as_bool).unwrap_or(false),
            status_code: args
                .get("status_code")
                .and_then(Value::as_u64)
                .map(|v| v as u16),
            session_id: arg_string(args, "session_id"),
            request_id: arg_string(args, "request_id"),
            source_file: None,
            raw_json: Some(args.to_string()),
            notes: arg_string(args, "notes"),
        };
        if record.total_tokens == 0 {
            record.total_tokens = record.input_tokens
                + record.output_tokens
                + record.cache_read_tokens
                + record.cache_creation_tokens;
        }
        self.pricing.calculate_cost(&mut record);
        let id = self.storage.insert_record(&record)?;
        Ok(json!({
            "id": id,
            "total_tokens": record.total_tokens,
            "cost_usd": record.cost_usd,
            "cost_cny": record.cost_cny,
        }))
    }
}

/// 解析 `last: "7d"` 风格的时间窗为 ISO 起始时间（与 Web/CLI 口径一致）。
fn parse_last(args: &Value) -> Option<String> {
    args.get("last").and_then(Value::as_str).map(|s| {
        let days: i64 = s.trim_end_matches('d').parse().unwrap_or(7);
        (Utc::now() - chrono::Duration::days(days))
            .format("%Y-%m-%dT%H:%M:%S%:z")
            .to_string()
    })
}

fn arg_string(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}

fn arg_u64(args: &Value, key: &str) -> u64 {
    args.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn tools_list() -> Value {
    let last_prop = json!({
        "type": "string",
        "description": "Time range like \"7d\" or \"30d\"; omit for all time"
    });
    json!({
        "tools": [
            {
                "name": "get_overview",
                "description": "Aggregate token usage & cost stats: requests, input/output/cache/reasoning tokens, cache hit rate, cost in USD and CNY.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "last": last_prop.clone() }
                }
            },
            {
                "name": "get_stats",
                "description": "Usage grouped by a dimension (requests, tokens, cost, cache hit rate per group).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "by": {
                            "type": "string",
                            "enum": ["provider", "model", "tool"],
                            "description": "Grouping dimension (default: provider)"
                        },
                        "last": last_prop.clone()
                    }
                }
            },
            {
                "name": "list_requests",
                "description": "Recent request-level usage records with optional filters (max 100).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "last": last_prop,
                        "limit": { "type": "integer", "description": "Max records to return (default 20, max 100)" },
                        "provider": { "type": "string" },
                        "model": { "type": "string" },
                        "tool": { "type": "string" }
                    }
                }
            },
            {
                "name": "get_budget_status",
                "description": "Monthly budget config and current month-to-date spend with usage percentage.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "report_usage",
                "description": "Push a usage record into AllTokens (Layer 3 ingestion). Cost is computed from the pricing table unless cost_usd is supplied.",
                "inputSchema": {
                    "type": "object",
                    "required": ["provider", "model"],
                    "properties": {
                        "provider": { "type": "string", "description": "e.g. openai, anthropic, deepseek" },
                        "model": { "type": "string", "description": "e.g. gpt-4o, claude-sonnet-4-20250514" },
                        "timestamp": { "type": "string", "description": "RFC3339; default: now" },
                        "tool": { "type": "string", "description": "Tool name, e.g. \"My Agent\"" },
                        "input_tokens": { "type": "integer" },
                        "output_tokens": { "type": "integer" },
                        "reasoning_tokens": { "type": "integer" },
                        "cache_read_tokens": { "type": "integer" },
                        "cache_creation_tokens": { "type": "integer" },
                        "total_tokens": { "type": "integer", "description": "Default: sum of input/output/cache tokens" },
                        "cost_usd": { "type": "number", "description": "Precomputed cost; default: computed from pricing table" },
                        "latency_ms": { "type": "integer" },
                        "is_stream": { "type": "boolean" },
                        "status_code": { "type": "integer" },
                        "session_id": { "type": "string" },
                        "request_id": { "type": "string" },
                        "notes": { "type": "string" }
                    }
                }
            }
        ]
    })
}

/// Serve MCP over stdio: newline-delimited JSON-RPC in on stdin, responses
/// out on stdout. Returns when stdin closes (client disconnects).
pub async fn run_stdio(server: McpServer) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(response) = server.handle_message(trimmed) {
            stdout.write_all(response.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_server() -> McpServer {
        McpServer::new(Storage::memory().unwrap()).unwrap()
    }

    fn call_tool(server: &McpServer, id: i64, name: &str, args: Value) -> Value {
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": args }
        });
        let resp = server.handle_message(&msg.to_string()).unwrap();
        serde_json::from_str(&resp).unwrap()
    }

    #[test]
    fn initialize_returns_protocol_and_server_info() {
        let server = make_server();
        let resp = server
            .handle_message(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#)
            .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["id"], 1);
        assert_eq!(v["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(v["result"]["serverInfo"]["name"], "alltokens");
        assert!(v["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn tools_list_exposes_five_tools_with_schemas() {
        let server = make_server();
        let resp = server
            .handle_message(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
            .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        let tools = v["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 5);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        for expected in [
            "get_overview",
            "get_stats",
            "list_requests",
            "get_budget_status",
            "report_usage",
        ] {
            assert!(names.contains(&expected), "missing tool {expected}");
        }
        assert!(tools.iter().all(|t| t["inputSchema"]["type"] == "object"));
    }

    #[test]
    fn report_usage_inserts_record_and_overview_reflects_it() {
        let server = make_server();
        let v = call_tool(
            &server,
            3,
            "report_usage",
            json!({
                "provider": "openai",
                "model": "gpt-4o",
                "input_tokens": 1_000_000,
                "output_tokens": 500_000,
                "tool": "MyTool",
                "session_id": "sess-1"
            }),
        );
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        let payload: Value = serde_json::from_str(text).unwrap();
        assert!(payload["id"].as_i64().unwrap() > 0);
        // gpt-4o: 1M input ($2.5/M) + 0.5M output ($10/M) = $7.5
        assert!((payload["cost_usd"].as_f64().unwrap() - 7.5).abs() < 1e-9);
        assert!(payload["cost_cny"].as_f64().unwrap() > 0.0);
        assert_eq!(payload["total_tokens"], 1_500_000);

        let page = server
            .storage
            .get_requests(&RequestFilter::default(), &Pagination { page: 0, page_size: 10 })
            .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].collector, "mcp");
        assert_eq!(page.items[0].tool.as_deref(), Some("MyTool"));

        let v = call_tool(&server, 4, "get_overview", json!({}));
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        let overview: Value = serde_json::from_str(text).unwrap();
        assert_eq!(overview["total_requests"], 1);
        assert_eq!(overview["total_tokens"], 1_500_000);
    }

    #[test]
    fn protocol_errors_and_notifications() {
        let server = make_server();
        // 未知方法 → -32601
        let resp = server
            .handle_message(r#"{"jsonrpc":"2.0","id":9,"method":"resources/list"}"#)
            .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["error"]["code"], -32601);
        // 未知工具 → -32602
        let v = call_tool(&server, 10, "nope", json!({}));
        assert_eq!(v["error"]["code"], -32602);
        // report_usage 缺必填字段 → isError 内容
        let v = call_tool(&server, 11, "report_usage", json!({ "model": "gpt-4o" }));
        assert_eq!(v["result"]["isError"], true);
        // 通知（无 id / id null）→ 无响应
        assert!(
            server
                .handle_message(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
                .is_none()
        );
        assert!(
            server
                .handle_message(r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#)
                .is_none()
        );
        // 非 JSON → -32700，id 为 null
        let resp = server.handle_message("not json").unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["error"]["code"], -32700);
        assert!(v["id"].is_null());
    }
}
