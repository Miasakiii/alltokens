// API Types — 与 alltokens-core/src/model.rs 对齐

export interface ApiResponse<T> {
  success: boolean;
  data: T;
}

export interface OverviewStats {
  total_requests: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_reasoning_tokens: number;
  total_cache_read_tokens: number;
  total_cache_creation_tokens: number;
  total_tokens: number;
  total_cost_usd: number;
  total_cost_cny: number;
  cache_hit_rate: number;
  success_rate: number;
}

export interface ProviderStats {
  provider: string;
  request_count: number;
  total_tokens: number;
  total_cost_usd: number;
  total_cost_cny: number;
  cache_hit_rate: number;
}

export interface ModelStats {
  provider: string;
  model: string;
  request_count: number;
  total_input: number;
  total_output: number;
  total_cache_read: number;
  total_cache_creation: number;
  total_tokens: number;
  total_cost_usd: number;
  total_cost_cny: number;
  cache_hit_rate: number;
}

export interface ToolStats {
  collector: string;
  tool: string | null;
  request_count: number;
  total_tokens: number;
  total_cost_usd: number;
  total_cost_cny: number;
}

export interface ProjectStats {
  project: string;
  request_count: number;
  total_tokens: number;
  total_cost_usd: number;
  total_cost_cny: number;
}

export interface SessionStats {
  session_id: string;
  provider: string;
  model: string;
  collector: string;
  request_count: number;
  total_input: number;
  total_output: number;
  total_tokens: number;
  total_cost_usd: number;
  total_cost_cny: number;
  first_seen: string;
  last_seen: string;
  duration_secs: number;
}

export interface ToolInvocationStats {
  name: string;
  invocation_count: number;
}

export interface SkillInvocationStats {
  name: string;
  invocation_count: number;
}

export interface DailySummary {
  date: string;
  provider: string;
  model: string;
  collector: string;
  request_count: number;
  total_input: number;
  total_output: number;
  total_cache_read: number;
  total_cache_creation: number;
  total_tokens: number;
  total_cost_usd: number;
  total_cost_cny: number;
  avg_latency_ms: number | null;
  cache_hit_rate: number;
}

export interface HeatmapDay {
  date: string;
  total_tokens: number;
  total_cost_usd: number;
  total_cost_cny: number;
  request_count: number;
}

export interface TokenHeatmap {
  period_days: number;
  start_date: string;
  end_date: string;
  days: HeatmapDay[];
}

export interface UsageRecord {
  id: number | null;
  timestamp: string;
  collector: string;
  tool: string | null;
  provider: string;
  model: string;
  input_tokens: number;
  output_tokens: number;
  reasoning_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  total_tokens: number;
  cost_usd: number;
  cost_cny: number;
  latency_ms: number | null;
  is_stream: boolean;
  status_code: number | null;
  session_id: string | null;
  request_id: string | null;
  source_file: string | null;
  raw_json: string | null;
  notes: string | null;
}

export interface PaginatedResult<T> {
  items: T[];
  total: number;
  page: number;
  page_size: number;
}

export interface BudgetConfig {
  monthly_usd: number | null;
  enabled: boolean;
}

export interface SubscriptionTier {
  label: string;
  monthly_usd: number;
}

export interface SubscriptionConfig {
  tiers: SubscriptionTier[];
  enabled: boolean;
}

export interface CaStatus {
  status: 'installed' | 'not_installed' | 'unknown';
  cert_present: boolean;
  cert_path: string;
  platform: string;
}

export interface PricingSummary {
  model_count: number;
  provider_count: number;
  usd_to_cny: number;
  overrides: PricingEntry[];
}

export interface PricingEntry {
  provider: string;
  model: string;
  input_per_mtok: number;
  output_per_mtok: number;
  cache_read_per_mtok: number;
  cache_create_per_mtok: number;
  context_window: number;
}

export interface PricingConfig {
  usd_to_cny: number | null;
  overrides: PricingEntry[];
}

export interface CollectorsConfig {
  enabled: Record<string, boolean>;
}

export interface GeneralConfig {
  auto_scan_interval_minutes: number;
  launch_at_startup: boolean;
}

export interface DataConfig {
  retention_days: number;
}

export interface CollectorStatus {
  id: string;
  name: string;
  available: boolean;
  enabled: boolean;
  last_scan_at: string | null;
}

export type CodexQuotaWindowKind = 'five_hour' | 'seven_day' | 'other';

export interface CodexQuotaWindow {
  kind: CodexQuotaWindowKind;
  used_percent: number | null;
  remaining_percent: number | null;
  window_duration_mins: number | null;
  resets_at: number | null;
}

export interface CodexQuotaSnapshot {
  fetched_at: string;
  source: string;
  plan_type: string | null;
  five_hour: CodexQuotaWindow | null;
  seven_day: CodexQuotaWindow | null;
  rate_limit_reached: boolean;
}

export interface CodexQuotaResponse {
  snapshot: CodexQuotaSnapshot | null;
  error: string | null;
}

export interface ClaudeQuotaSnapshot {
  fetched_at: string;
  source: string;
  snapshot_path: string | null;
  captured_at: string | null;
  is_stale: boolean;
  five_hour: CodexQuotaWindow | null;
  seven_day: CodexQuotaWindow | null;
}

export interface ClaudeQuotaResponse {
  snapshot: ClaudeQuotaSnapshot | null;
  error: string | null;
}

export interface ScanResult {
  total: number;
  by_collector: [string, number][];
}

/** Hour-of-week activity cell (weekday 0=Sunday..6, hour 0..23, server local time). */
export interface HourOfWeekCell {
  weekday: number;
  hour: number;
  total_tokens: number;
  request_count: number;
}
