//! Extract token usage from known API response formats.

use alltokens_core::model::{Provider, UsageRecord};
use chrono::Utc;
use serde_json::Value;

/// Known API host suffixes worth intercepting for usage extraction.
pub const INTERCEPT_HOSTS: &[&str] = &[
    "api.openai.com",
    "api.anthropic.com",
    "api.deepseek.com",
    "dashscope.aliyuncs.com",
    "api.moonshot.cn",
    "open.bigmodel.cn",
    "api.minimax.chat",
    "api.siliconflow.cn",
    "ark.cn-beijing.volces.com",
    "generativelanguage.googleapis.com",
    "api.x.ai",
    "api.groq.com",
    "api.mistral.ai",
    "api.stepfun.com",
    "api.baichuan-ai.com",
    "api.lingyiwanwu.com",
];

/// Returns true if the host should be inspected for usage data.
pub fn should_intercept_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    INTERCEPT_HOSTS.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

/// Parse usage from a complete HTTP response body (non-streaming).
pub fn extract_usage_from_body(host: &str, model_hint: Option<&str>, body: &str) -> Option<UsageRecord> {
    let val: Value = serde_json::from_str(body).ok()?;
    // OpenAI/Anthropic 用 `usage`；Google Gemini 用 `usageMetadata`。
    let usage = val
        .get("usage")
        .or_else(|| val.pointer("/message/usage"))
        .or_else(|| val.get("usageMetadata"))?;
    let model = val
        .get("model")
        .or_else(|| val.pointer("/message/model"))
        .or_else(|| val.get("modelVersion"))
        .and_then(|v| v.as_str())
        .or(model_hint)
        .unwrap_or("")
        .to_string();
    if model.is_empty() {
        return None;
    }

    let input = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .or_else(|| usage.get("promptTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .or_else(|| usage.get("candidatesTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_read = usage
        .get("cache_read_input_tokens")
        .or_else(|| usage.get("cached_tokens"))
        .or_else(|| usage.get("cache_read_tokens"))
        .or_else(|| usage.get("cachedContentTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_creation = usage
        .get("cache_creation_input_tokens")
        .or_else(|| usage.get("cache_creation_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let reasoning = usage
        .get("reasoning_tokens")
        .or_else(|| usage.get("reasoning_output_tokens"))
        .or_else(|| usage.pointer("/completion_tokens_details/reasoning_tokens"))
        .or_else(|| usage.get("thoughtsTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total = usage
        .get("total_tokens")
        .or_else(|| usage.get("totalTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(input + output + cache_read + cache_creation);

    if input == 0 && output == 0 {
        return None;
    }

    let provider = Provider::from_url_and_model(host, &model);
    Some(UsageRecord {
        id: None,
        timestamp: Utc::now(),
        collector: "proxy".to_string(),
        tool: Some("Transparent Proxy".to_string()),
        provider: provider.name().to_string(),
        model,
        input_tokens: input,
        output_tokens: output,
        reasoning_tokens: reasoning,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_creation,
        total_tokens: total,
        cost_usd: 0.0,
        cost_cny: 0.0,
        latency_ms: None,
        is_stream: false,
        status_code: Some(200),
        session_id: None,
        request_id: val
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        source_file: None,
        raw_json: Some(body.to_string()),
        notes: None,
    })
}

/// Accumulate usage from SSE `data:` lines (OpenAI / Anthropic streaming).
pub fn extract_usage_from_sse(host: &str, model_hint: Option<&str>, sse_body: &str) -> Option<UsageRecord> {
    let mut last_usage: Option<Value> = None;
    let mut model = model_hint.map(|s| s.to_string());

    for line in sse_body.lines() {
        let line = line.trim();
        if !line.starts_with("data:") {
            continue;
        }
        let payload = line.trim_start_matches("data:").trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<Value>(payload) {
            if let Some(m) = val
                .get("model")
                .or_else(|| val.get("modelVersion"))
                .and_then(|v| v.as_str())
            {
                model = Some(m.to_string());
            }
            if val.get("usage").is_some() {
                last_usage = val.get("usage").cloned();
            }
            if let Some(u) = val.pointer("/message/usage") {
                last_usage = Some(u.clone());
            }
            if let Some(u) = val.get("usageMetadata") {
                last_usage = Some(u.clone());
            }
        }
    }

    let usage = last_usage?;
    let model = model?;
    let input = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .or_else(|| usage.get("promptTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .or_else(|| usage.get("candidatesTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if input == 0 && output == 0 {
        return None;
    }
    let cache_read = usage
        .get("cache_read_input_tokens")
        .or_else(|| usage.get("cached_tokens"))
        .or_else(|| usage.get("cache_read_tokens"))
        .or_else(|| usage.get("cachedContentTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_creation = usage
        .get("cache_creation_input_tokens")
        .or_else(|| usage.get("cache_creation_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let reasoning = usage
        .get("reasoning_tokens")
        .or_else(|| usage.get("reasoning_output_tokens"))
        .or_else(|| usage.pointer("/completion_tokens_details/reasoning_tokens"))
        .or_else(|| usage.get("thoughtsTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total = usage
        .get("total_tokens")
        .or_else(|| usage.get("totalTokenCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(input + output);

    let provider = Provider::from_url_and_model(host, &model);
    Some(UsageRecord {
        id: None,
        timestamp: Utc::now(),
        collector: "proxy".to_string(),
        tool: Some("Transparent Proxy".to_string()),
        provider: provider.name().to_string(),
        model,
        input_tokens: input,
        output_tokens: output,
        reasoning_tokens: reasoning,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_creation,
        total_tokens: total,
        cost_usd: 0.0,
        cost_cny: 0.0,
        latency_ms: None,
        is_stream: true,
        status_code: Some(200),
        session_id: None,
        request_id: None,
        source_file: None,
        raw_json: Some(usage.to_string()),
        notes: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intercepts_known_hosts() {
        assert!(should_intercept_host("api.openai.com"));
        assert!(should_intercept_host("api.anthropic.com"));
        assert!(should_intercept_host("generativelanguage.googleapis.com"));
        assert!(should_intercept_host("api.groq.com"));
        assert!(should_intercept_host("api.mistral.ai"));
        assert!(should_intercept_host("api.x.ai"));
        assert!(!should_intercept_host("example.com"));
    }

    #[test]
    fn extracts_openai_chat_completion_usage() {
        let body = r#"{"id":"chatcmpl-abc","model":"gpt-4o","usage":{"prompt_tokens":100,"completion_tokens":50,"total_tokens":150}}"#;
        let record = extract_usage_from_body("api.openai.com", None, body).unwrap();
        assert_eq!(record.input_tokens, 100);
        assert_eq!(record.output_tokens, 50);
        assert_eq!(record.provider, "OpenAI");
        assert_eq!(record.collector, "proxy");
    }

    #[test]
    fn extracts_anthropic_message_usage() {
        let body = r#"{"id":"msg_123","model":"claude-sonnet-4-20250514","usage":{"input_tokens":200,"output_tokens":80,"cache_read_input_tokens":50,"cache_creation_input_tokens":10}}"#;
        let record = extract_usage_from_body("api.anthropic.com", None, body).unwrap();
        assert_eq!(record.cache_read_tokens, 50);
        assert_eq!(record.cache_creation_tokens, 10);
        assert_eq!(record.provider, "Anthropic");
    }

    #[test]
    fn extracts_qwen_dashscope_usage() {
        let body = r#"{"model":"qwen-plus","usage":{"input_tokens":1000,"output_tokens":400,"total_tokens":1400}}"#;
        let record = extract_usage_from_body("dashscope.aliyuncs.com", None, body).unwrap();
        assert_eq!(record.provider, "Qwen");
    }

    #[test]
    fn extracts_openai_sse_final_chunk() {
        let sse = "data: {\"model\":\"gpt-4o-mini\",\"choices\":[]}\n\
                   data: {\"model\":\"gpt-4o-mini\",\"usage\":{\"prompt_tokens\":30,\"completion_tokens\":15,\"total_tokens\":45}}\n\
                   data: [DONE]\n";
        let record = extract_usage_from_sse("api.openai.com", None, sse).unwrap();
        assert_eq!(record.input_tokens, 30);
        assert!(record.is_stream);
    }

    #[test]
    fn sse_preserves_reasoning_tokens() {
        let sse = "data: {\"model\":\"o3-mini\",\"usage\":{\"prompt_tokens\":40,\"completion_tokens\":120,\"total_tokens\":160,\"completion_tokens_details\":{\"reasoning_tokens\":88}}}\n\
                   data: [DONE]\n";
        let record = extract_usage_from_sse("api.openai.com", None, sse).unwrap();
        assert_eq!(record.reasoning_tokens, 88);
        assert_eq!(record.total_tokens, 160);
        assert!(record.is_stream);
    }

    #[test]
    fn extracts_gemini_usage_metadata() {
        // Google generateContent 真实形态：usageMetadata + modelVersion
        let body = r#"{"modelVersion":"gemini-2.0-flash","candidates":[],"usageMetadata":{"promptTokenCount":1200,"candidatesTokenCount":300,"totalTokenCount":1650,"cachedContentTokenCount":150,"thoughtsTokenCount":50}}"#;
        let record =
            extract_usage_from_body("generativelanguage.googleapis.com", None, body).unwrap();
        assert_eq!(record.input_tokens, 1200);
        assert_eq!(record.output_tokens, 300);
        assert_eq!(record.cache_read_tokens, 150);
        assert_eq!(record.reasoning_tokens, 50);
        assert_eq!(record.total_tokens, 1650);
        assert_eq!(record.model, "gemini-2.0-flash");
        assert_eq!(record.provider, "Google");
    }

    #[test]
    fn extracts_gemini_sse_usage_metadata() {
        let sse = "data: {\"candidates\":[],\"modelVersion\":\"gemini-2.5-pro\"}\n\
                   data: {\"modelVersion\":\"gemini-2.5-pro\",\"usageMetadata\":{\"promptTokenCount\":800,\"candidatesTokenCount\":160,\"totalTokenCount\":960}}\n";
        let record =
            extract_usage_from_sse("generativelanguage.googleapis.com", None, sse).unwrap();
        assert_eq!(record.input_tokens, 800);
        assert_eq!(record.output_tokens, 160);
        assert_eq!(record.provider, "Google");
        assert!(record.is_stream);
    }

    #[test]
    fn gemini_body_without_model_uses_hint() {
        let body = r#"{"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5,"totalTokenCount":15}}"#;
        let record = extract_usage_from_body(
            "generativelanguage.googleapis.com",
            Some("gemini-2.0-flash-lite"),
            body,
        )
        .unwrap();
        assert_eq!(record.model, "gemini-2.0-flash-lite");
    }
}
