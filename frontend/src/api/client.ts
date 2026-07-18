import type { ApiResponse, OverviewStats, ProviderStats, ModelStats, ToolStats, ProjectStats, SessionStats, ToolInvocationStats, SkillInvocationStats, DailySummary, TokenHeatmap, HourOfWeekCell, UsageRecord, PaginatedResult, BudgetConfig, SubscriptionConfig, CaStatus, PricingSummary, PricingEntry, PricingConfig, CollectorsConfig, GeneralConfig, DataConfig, CollectorStatus, CodexQuotaResponse, ClaudeQuotaResponse, ScanResult } from './types';

const BASE = '/api';

/** Tauri 桌面端窗口以 tauri://localhost / tauri.localhost 源加载静态资源，
 *  同源 /api 不存在，需指向内嵌 Web 服务（127.0.0.1:3212）。 */
const IS_TAURI =
  window.location.protocol === 'tauri:' || window.location.hostname === 'tauri.localhost';
const API_BASE = IS_TAURI ? 'http://127.0.0.1:3212/api' : BASE;

export function isTauriApp(): boolean {
  return IS_TAURI;
}

export type ExportFormat = 'csv' | 'json' | 'pdf';

function buildUrl(path: string, params?: Record<string, string | undefined>): URL {
  const url = new URL(path, window.location.origin);
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      if (v !== undefined && v !== '') url.searchParams.set(k, v);
    }
  }
  return url;
}

async function get<T>(path: string, params?: Record<string, string | undefined>): Promise<T> {
  const url = buildUrl(path, params);
  const res = await fetch(url.toString());
  if (!res.ok) throw new Error(`API error: ${res.status}`);
  const json: ApiResponse<T> = await res.json();
  return json.data;
}

function downloadBlob(blob: Blob, filename: string) {
  const link = document.createElement('a');
  link.href = URL.createObjectURL(blob);
  link.download = filename;
  link.click();
  URL.revokeObjectURL(link.href);
}

export interface StatsQuery {
  provider?: string;
  model?: string;
  collector?: string;
  tool?: string;
  start_date?: string;
  end_date?: string;
  last?: string;
  days?: number;
}

export interface ListQuery extends StatsQuery {
  page?: number;
  page_size?: number;
}

export const api = {
  health: () => fetch(`${API_BASE}/health`).then(r => r.json()),

  async scan(): Promise<ScanResult> {
    const res = await fetch(`${API_BASE}/scan`, { method: 'POST' });
    if (!res.ok) throw new Error(`API error: ${res.status}`);
    const json: ApiResponse<ScanResult> = await res.json();
    return json.data;
  },

  overview: (q?: StatsQuery) => get<OverviewStats>(`${API_BASE}/overview`, q as Record<string, string>),
  providers: (q?: StatsQuery) => get<ProviderStats[]>(`${API_BASE}/providers`, q as Record<string, string>),
  models: (q?: StatsQuery) => get<ModelStats[]>(`${API_BASE}/models`, q as Record<string, string>),
  tools: (q?: StatsQuery) => get<ToolStats[]>(`${API_BASE}/tools`, q as Record<string, string>),
  toolsRanking: (q?: StatsQuery) => get<ToolInvocationStats[]>(`${API_BASE}/tools/ranking`, q as Record<string, string>),
  skillsRanking: (q?: StatsQuery) => get<SkillInvocationStats[]>(`${API_BASE}/skills/ranking`, q as Record<string, string>),
  projects: (q?: StatsQuery) => get<ProjectStats[]>(`${API_BASE}/projects`, q as Record<string, string>),
  sessions: (q?: StatsQuery) => get<SessionStats[]>(`${API_BASE}/sessions`, q as Record<string, string>),
  trends: (q?: StatsQuery) => get<DailySummary[]>(`${API_BASE}/trends`, q as Record<string, string>),
  heatmap: (q?: StatsQuery) => get<TokenHeatmap>(`${API_BASE}/heatmap`, q as Record<string, string>),
  hourOfWeek: (q?: StatsQuery) => get<HourOfWeekCell[]>(`${API_BASE}/heatmap/hourly`, q as Record<string, string>),
  requests: (q?: ListQuery) => get<PaginatedResult<UsageRecord>>(`${API_BASE}/requests`, q as Record<string, string>),

  budgetConfig: () => get<BudgetConfig>(`${API_BASE}/config/budget`),

  subscriptionConfig: () => get<SubscriptionConfig>(`${API_BASE}/config/subscription`),

  caStatus: () => get<CaStatus>(`${API_BASE}/ca/status`),

  async installCa(): Promise<CaStatus> {
    const res = await fetch(`${API_BASE}/ca/install`, { method: 'POST' });
    if (!res.ok) throw new Error(`API error: ${res.status}`);
    const json: ApiResponse<CaStatus> = await res.json();
    return json.data;
  },

  async uninstallCa(): Promise<CaStatus> {
    const res = await fetch(`${API_BASE}/ca/uninstall`, { method: 'POST' });
    if (!res.ok) throw new Error(`API error: ${res.status}`);
    const json: ApiResponse<CaStatus> = await res.json();
    return json.data;
  },

  pricingSummary: () => get<PricingSummary>(`${API_BASE}/config/pricing`),

  pricingModels: () => get<PricingEntry[]>(`${API_BASE}/pricing/models`),

  collectors: () => get<CollectorStatus[]>(`${API_BASE}/collectors`),

  codexQuota: (refresh = false) =>
    get<CodexQuotaResponse>(`${API_BASE}/quota/codex`, refresh ? { refresh: 'true' } : undefined),

  claudeQuota: (refresh = false) =>
    get<ClaudeQuotaResponse>(`${API_BASE}/quota/claude`, refresh ? { refresh: 'true' } : undefined),

  generalConfig: () => get<GeneralConfig>(`${API_BASE}/config/general`),

  dataConfig: () => get<DataConfig>(`${API_BASE}/config/data`),

  async setPricingConfig(config: PricingConfig): Promise<PricingSummary> {
    const res = await fetch(`${API_BASE}/config/pricing`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(config),
    });
    if (!res.ok) throw new Error(`API error: ${res.status}`);
    const json: ApiResponse<PricingSummary> = await res.json();
    return json.data;
  },

  async setCollectorsConfig(config: CollectorsConfig): Promise<CollectorStatus[]> {
    const res = await fetch(`${API_BASE}/config/collectors`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(config),
    });
    if (!res.ok) throw new Error(`API error: ${res.status}`);
    const json: ApiResponse<CollectorStatus[]> = await res.json();
    return json.data;
  },

  async setGeneralConfig(config: GeneralConfig): Promise<GeneralConfig> {
    const res = await fetch(`${API_BASE}/config/general`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(config),
    });
    if (!res.ok) throw new Error(`API error: ${res.status}`);
    const json: ApiResponse<GeneralConfig> = await res.json();
    return json.data;
  },

  async setDataConfig(config: DataConfig): Promise<DataConfig> {
    const res = await fetch(`${API_BASE}/config/data`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(config),
    });
    if (!res.ok) throw new Error(`API error: ${res.status}`);
    const json: ApiResponse<DataConfig> = await res.json();
    return json.data;
  },

  async setBudgetConfig(config: BudgetConfig): Promise<BudgetConfig> {
    const res = await fetch(`${API_BASE}/config/budget`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(config),
    });
    if (!res.ok) throw new Error(`API error: ${res.status}`);
    const json: ApiResponse<BudgetConfig> = await res.json();
    return json.data;
  },

  async setSubscriptionConfig(config: SubscriptionConfig): Promise<SubscriptionConfig> {
    const res = await fetch(`${API_BASE}/config/subscription`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(config),
    });
    if (!res.ok) throw new Error(`API error: ${res.status}`);
    const json: ApiResponse<SubscriptionConfig> = await res.json();
    return json.data;
  },

  async exportData(format: ExportFormat, q?: StatsQuery): Promise<void> {
    const url = buildUrl(`${API_BASE}/export`, {
      format,
      ...(q as Record<string, string | undefined>),
    });
    // PDF is a print-ready HTML report: open in a new tab so the browser
    // renders it and auto-invokes the print dialog ("Save as PDF").
    if (format === 'pdf') {
      window.open(url.toString(), '_blank', 'noopener');
      return;
    }
    const res = await fetch(url.toString());
    if (!res.ok) throw new Error(`Export failed: ${res.status}`);

    const disposition = res.headers.get('Content-Disposition') ?? '';
    const match = disposition.match(/filename="([^"]+)"/);
    const filename = match?.[1] ?? `alltokens-export.${format}`;
    const blob = await res.blob();
    downloadBlob(blob, filename);
  },
};
