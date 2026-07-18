use crate::model::UsageRecord;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// 单条定价规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingEntry {
    pub provider: String,
    pub model: String,
    /// 每百万 input token (USD)
    pub input_per_mtok: f64,
    /// 每百万 output token (USD)
    pub output_per_mtok: f64,
    /// 每百万缓存读取 token (USD)
    pub cache_read_per_mtok: f64,
    /// 每百万缓存创建 token (USD)
    pub cache_create_per_mtok: f64,
    /// 模型上下文窗口大小 (tokens)，0 = 未知
    #[serde(default)]
    pub context_window: u64,
}

/// 内置定价表 (TOML 格式)
#[derive(Debug, Serialize, Deserialize)]
struct PricingToml {
    #[serde(flatten)]
    providers: HashMap<String, HashMap<String, ModelPricing>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ModelPricing {
    input: f64,
    output: f64,
    #[serde(default)]
    cache_read: f64,
    #[serde(default)]
    cache_create: f64,
    #[serde(default)]
    context_window: u64,
}

/// 定价引擎
pub struct PricingEngine {
    /// key = "provider/model"
    entries: HashMap<String, PricingEntry>,
    /// USD -> CNY 汇率
    usd_to_cny: f64,
}

impl PricingEngine {
    /// 从内置 TOML 加载
    pub fn from_builtin() -> Self {
        let toml_str = include_str!("../../../pricing/builtin.toml");
        Self::from_toml(toml_str).unwrap_or_else(|e| {
            tracing::warn!("Failed to load builtin pricing: {e}, using empty pricing");
            Self::new()
        })
    }

    /// 从 TOML 字符串解析
    pub fn from_toml(toml_str: &str) -> Result<Self, String> {
        let parsed: PricingToml =
            toml::from_str(toml_str).map_err(|e| format!("Failed to parse pricing TOML: {e}"))?;

        let mut entries = HashMap::new();
        for (provider, models) in &parsed.providers {
            for (model, pricing) in models {
                let key = format!("{provider}/{model}");
                entries.insert(
                    key,
                    PricingEntry {
                        provider: provider.clone(),
                        model: model.clone(),
                        input_per_mtok: pricing.input,
                        output_per_mtok: pricing.output,
                        cache_read_per_mtok: pricing.cache_read,
                        cache_create_per_mtok: pricing.cache_create,
                        context_window: pricing.context_window,
                    },
                );
            }
        }

        Ok(Self {
            entries,
            usd_to_cny: 7.25, // 默认汇率
        })
    }

    /// 从文件加载
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read pricing file {}: {e}", path.display()))?;
        Self::from_toml(&content)
    }

    /// 空定价引擎
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            usd_to_cny: 7.25,
        }
    }

    /// 设置汇率
    pub fn set_usd_to_cny(&mut self, rate: f64) {
        self.usd_to_cny = rate;
    }

    /// 合并用户自定义定价 (覆盖内置)
    pub fn merge_user_pricing(&mut self, user_entries: Vec<PricingEntry>) {
        for entry in user_entries {
            let key = format!("{}/{}", entry.provider, entry.model);
            self.entries.insert(key, entry);
        }
    }

    /// 查找定价
    pub fn find(&self, provider: &str, model: &str) -> Option<&PricingEntry> {
        // 精确匹配
        let key = format!("{provider}/{model}");
        if let Some(entry) = self.entries.get(&key) {
            return Some(entry);
        }

        // 模糊匹配 (去掉版本号后缀)
        // 例如 "claude-sonnet-4-20250514" -> 尝试 "claude-sonnet-4"
        // 用 rsplit_once 仅剥离最后一个 '-' 段并保持原有顺序
        // （旧实现用 rsplit().skip(1).join() 会把段序反转成 "4-sonnet-claude"，永不命中）
        if let Some((model_base, _)) = model.rsplit_once('-') {
            let key = format!("{provider}/{model_base}");
            if let Some(entry) = self.entries.get(&key) {
                return Some(entry);
            }
        }

        None
    }

    /// 计算一条记录的成本 (如果没有手动设置的话)
    pub fn calculate_cost(&self, record: &mut UsageRecord) {
        if record.cost_usd > 0.0 {
            // 已有成本，只换算 CNY
            if record.cost_cny == 0.0 {
                record.cost_cny = record.cost_usd * self.usd_to_cny;
            }
            return;
        }

        if let Some(pricing) = self.find(&record.provider, &record.model) {
            let cost = (record.input_tokens as f64 / 1_000_000.0) * pricing.input_per_mtok
                + (record.output_tokens as f64 / 1_000_000.0) * pricing.output_per_mtok
                + (record.cache_read_tokens as f64 / 1_000_000.0) * pricing.cache_read_per_mtok
                + (record.cache_creation_tokens as f64 / 1_000_000.0) * pricing.cache_create_per_mtok;

            record.cost_usd = (cost * 10000.0).round() / 10000.0; // 4 位小数
            record.cost_cny = (cost * self.usd_to_cny * 10000.0).round() / 10000.0;
        }
    }

    /// 获取所有定价条目
    pub fn all_entries(&self) -> Vec<&PricingEntry> {
        self.entries.values().collect()
    }

    /// 查找模型上下文窗口大小 (tokens)。复用 `find` 的精确 + 模糊匹配，未知返回 None。
    pub fn context_window(&self, provider: &str, model: &str) -> Option<u64> {
        self.find(provider, model)
            .map(|e| e.context_window)
            .filter(|&cw| cw > 0)
    }

    /// Number of loaded model pricing entries.
    pub fn model_count(&self) -> usize {
        self.entries.len()
    }

    /// Distinct provider count in the pricing table.
    pub fn provider_count(&self) -> usize {
        let mut providers: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for entry in self.entries.values() {
            providers.insert(&entry.provider);
        }
        providers.len()
    }

    /// USD → CNY conversion rate.
    pub fn usd_to_cny(&self) -> f64 {
        self.usd_to_cny
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    const SAMPLE_TOML: &str = r#"
[openai."gpt-4o"]
input = 2.50
output = 10.00
cache_read = 1.25

[anthropic."claude-sonnet-4"]
input = 3.00
output = 15.00
cache_read = 0.30
cache_create = 3.75
"#;

    fn sample_record(provider: &str, model: &str, input: u64, output: u64, cache_read: u64) -> UsageRecord {
        UsageRecord {
            id: None,
            timestamp: Utc::now(),
            collector: "test".to_string(),
            tool: None,
            provider: provider.to_string(),
            model: model.to_string(),
            input_tokens: input,
            output_tokens: output,
            reasoning_tokens: 0,
            cache_read_tokens: cache_read,
            cache_creation_tokens: 0,
            total_tokens: input + output + cache_read,
            cost_usd: 0.0,
            cost_cny: 0.0,
            latency_ms: None,
            is_stream: false,
            status_code: None,
            session_id: None,
            request_id: None,
            source_file: None,
            raw_json: None,
            notes: None,
        }
    }

    #[test]
    fn parse_toml_into_entries() {
        let engine = PricingEngine::from_toml(SAMPLE_TOML).unwrap();
        let entry = engine.find("openai", "gpt-4o").unwrap();
        assert!((entry.input_per_mtok - 2.50).abs() < f64::EPSILON);
        assert!((entry.output_per_mtok - 10.00).abs() < f64::EPSILON);
        assert!((entry.cache_read_per_mtok - 1.25).abs() < f64::EPSILON);
    }

    #[test]
    fn find_exact_model_match() {
        let engine = PricingEngine::from_toml(SAMPLE_TOML).unwrap();
        assert!(engine.find("anthropic", "claude-sonnet-4").is_some());
        assert!(engine.find("anthropic", "unknown-model").is_none());
    }

    #[test]
    fn find_fuzzy_strips_dated_version_suffix() {
        // Dated model IDs should fall back to the un-versioned base entry.
        // Regression: the old rsplit().skip(1).join() reversed the segments
        // into "4-sonnet-claude" and never matched.
        let engine = PricingEngine::from_toml(SAMPLE_TOML).unwrap();
        let entry = engine
            .find("anthropic", "claude-sonnet-4-20250514")
            .expect("dated suffix should fall back to claude-sonnet-4");
        assert_eq!(entry.model, "claude-sonnet-4");
        assert!((entry.input_per_mtok - 3.00).abs() < f64::EPSILON);
    }

    #[test]
    fn calculate_cost_from_token_usage() {
        let mut engine = PricingEngine::from_toml(SAMPLE_TOML).unwrap();
        engine.set_usd_to_cny(7.0);

        let mut record = sample_record("openai", "gpt-4o", 1_000_000, 1_000_000, 0);
        engine.calculate_cost(&mut record);

        // 1M input @ $2.50 + 1M output @ $10.00 = $12.50
        assert!((record.cost_usd - 12.50).abs() < 0.001);
        assert!((record.cost_cny - 87.50).abs() < 0.001);
    }

    #[test]
    fn calculate_cost_with_cache_tokens() {
        let engine = PricingEngine::from_toml(SAMPLE_TOML).unwrap();
        let mut record = sample_record("anthropic", "claude-sonnet-4", 0, 0, 1_000_000);
        engine.calculate_cost(&mut record);
        assert!((record.cost_usd - 0.30).abs() < 0.001);
    }

    #[test]
    fn preserves_existing_usd_and_converts_cny() {
        let mut engine = PricingEngine::from_toml(SAMPLE_TOML).unwrap();
        engine.set_usd_to_cny(8.0);

        let mut record = sample_record("openai", "gpt-4o", 1000, 500, 0);
        record.cost_usd = 1.25;
        engine.calculate_cost(&mut record);

        assert!((record.cost_usd - 1.25).abs() < f64::EPSILON);
        assert!((record.cost_cny - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn merge_user_pricing_overrides_builtin() {
        let mut engine = PricingEngine::from_toml(SAMPLE_TOML).unwrap();
        engine.merge_user_pricing(vec![PricingEntry {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            input_per_mtok: 1.0,
            output_per_mtok: 2.0,
            cache_read_per_mtok: 0.0,
            cache_create_per_mtok: 0.0,
            context_window: 0,
        }]);

        let entry = engine.find("openai", "gpt-4o").unwrap();
        assert!((entry.input_per_mtok - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn context_window_parsed_and_looked_up() {
        let toml_str = r#"
[openai."gpt-4o"]
input = 2.5
output = 10.0
context_window = 128000

[anthropic."claude-sonnet-4"]
input = 3.0
output = 15.0
context_window = 200000
"#;
        let engine = PricingEngine::from_toml(toml_str).unwrap();
        assert_eq!(engine.find("openai", "gpt-4o").unwrap().context_window, 128000);
        assert_eq!(engine.context_window("openai", "gpt-4o"), Some(128000));
        assert_eq!(engine.context_window("anthropic", "claude-sonnet-4"), Some(200000));
    }

    #[test]
    fn context_window_absent_returns_none() {
        let toml_str = r#"
[openai."gpt-legacy"]
input = 1.0
output = 1.0
"#;
        let engine = PricingEngine::from_toml(toml_str).unwrap();
        assert_eq!(engine.find("openai", "gpt-legacy").unwrap().context_window, 0);
        assert_eq!(engine.context_window("openai", "gpt-legacy"), None);
    }

    #[test]
    fn from_builtin_loads_pricing_entries() {
        let engine = PricingEngine::from_builtin();
        assert!(!engine.all_entries().is_empty());
        assert!(engine.find("deepseek", "deepseek-chat").is_some());
    }
}
