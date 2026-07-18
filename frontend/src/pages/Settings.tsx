import { useState, useEffect } from 'react';
import type { ReactNode } from 'react';
import Layout from '../components/Layout';
import { Card, LoadingRows, Skeleton } from '../components/ui/primitives';
import { api, type StatsQuery } from '../api/client';
import type { BudgetConfig, SubscriptionConfig, SubscriptionTier, PricingSummary, PricingEntry } from '../api/types';
import { useBudgetConfig } from '../hooks/useBudget';
import { useSubscriptionConfig } from '../hooks/useSubscription';
import { useCa } from '../hooks/useCa';
import { useTheme } from '../hooks/useTheme';
import { useLang } from '../i18n';

const PERIODS = [
  { en: 'All time', zh: '全部时间', value: '' },
  { en: '7 days', zh: '近 7 天', value: '7d' },
  { en: '30 days', zh: '近 30 天', value: '30d' },
  { en: '90 days', zh: '近 90 天', value: '90d' },
];

function Section({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children?: ReactNode;
}) {
  const zh = useLang().lang === 'zh';
  return (
    <Card title={title} className="min-w-0">
      <p className="text-xs text-muted mb-4">{description}</p>
      {children ?? (
        <div
          className="surface-2 px-4 py-6 text-center text-sm text-faint"
          style={{ borderStyle: 'dashed' }}
        >
          {zh ? '敬请期待' : 'Coming soon'}
        </div>
      )}
    </Card>
  );
}

const CHECKBOX_CLS = 'h-3.5 w-3.5 shrink-0 accent-[var(--app-accent)]';

function DataExport() {
  const zh = useLang().lang === 'zh';
  const [period, setPeriod] = useState('30d');
  const [retentionDays, setRetentionDays] = useState(0);
  const [retentionLoading, setRetentionLoading] = useState(true);
  const [retentionSaving, setRetentionSaving] = useState(false);
  const [retentionMessage, setRetentionMessage] = useState<string | null>(null);
  const [exporting, setExporting] = useState<'csv' | 'json' | 'pdf' | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .dataConfig()
      .then((c) => setRetentionDays(c.retention_days))
      .catch((e) => setError(e.message))
      .finally(() => setRetentionLoading(false));
  }, []);

  const query: StatsQuery = period ? { last: period } : {};

  const handleExport = async (format: 'csv' | 'json' | 'pdf') => {
    setExporting(format);
    setError(null);
    try {
      await api.exportData(format, query);
    } catch (err) {
      setError(err instanceof Error ? err.message : zh ? '导出失败' : 'Export failed');
    } finally {
      setExporting(null);
    }
  };

  const handleSaveRetention = async () => {
    setRetentionSaving(true);
    setRetentionMessage(null);
    setError(null);
    try {
      await api.setDataConfig({ retention_days: retentionDays });
      setRetentionMessage(
        retentionDays === 0
          ? zh
            ? '保留策略已停用 — 保留全部记录'
            : 'Retention disabled — all records kept'
          : zh
            ? `保留策略已保存 — 早于 ${retentionDays} 天的记录将被删除`
            : `Retention saved — records older than ${retentionDays} days removed`,
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : zh ? '保存保留策略失败' : 'Failed to save retention');
    } finally {
      setRetentionSaving(false);
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <label htmlFor="export-period" className="label-xs block mb-1.5">
          {zh ? '时间范围' : 'Time range'}
        </label>
        <select
          id="export-period"
          value={period}
          onChange={(e) => setPeriod(e.target.value)}
          className="input w-full"
        >
          {PERIODS.map((p) => (
            <option key={p.value || 'all'} value={p.value}>
              {zh ? p.zh : p.en}
            </option>
          ))}
        </select>
      </div>

      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={() => handleExport('csv')}
          disabled={exporting !== null}
          className="btn"
        >
          {exporting === 'csv' ? (zh ? '导出中…' : 'Exporting…') : zh ? '导出 CSV' : 'Export CSV'}
        </button>
        <button
          type="button"
          onClick={() => handleExport('json')}
          disabled={exporting !== null}
          className="btn"
        >
          {exporting === 'json' ? (zh ? '导出中…' : 'Exporting…') : zh ? '导出 JSON' : 'Export JSON'}
        </button>
        <button
          type="button"
          onClick={() => handleExport('pdf')}
          disabled={exporting !== null}
          className="btn"
        >
          {exporting === 'pdf' ? (zh ? '打开中…' : 'Opening…') : zh ? '导出 PDF' : 'Export PDF'}
        </button>
      </div>

      <div>
        <label htmlFor="retention-days" className="label-xs block mb-1.5">
          {zh ? '数据保留' : 'Data retention'}
        </label>
        {retentionLoading ? (
          <Skeleton className="h-8 w-full" />
        ) : (
          <select
            id="retention-days"
            value={retentionDays}
            onChange={(e) => setRetentionDays(Number(e.target.value))}
            className="input w-full"
          >
            <option value={0}>{zh ? '保留全部记录' : 'Keep all records'}</option>
            <option value={30}>{zh ? '删除 30 天前的记录' : 'Delete records older than 30 days'}</option>
            <option value={90}>{zh ? '删除 90 天前的记录' : 'Delete records older than 90 days'}</option>
            <option value={180}>{zh ? '删除 180 天前的记录' : 'Delete records older than 180 days'}</option>
            <option value={365}>{zh ? '删除 1 年前的记录' : 'Delete records older than 1 year'}</option>
          </select>
        )}
      </div>

      <button
        type="button"
        onClick={handleSaveRetention}
        disabled={retentionSaving || retentionLoading}
        className="btn btn-primary"
      >
        {retentionSaving ? (zh ? '保存中…' : 'Saving…') : zh ? '保存保留策略' : 'Save retention'}
      </button>

      {retentionMessage && <p className="text-xs text-success">{retentionMessage}</p>}
      {error && <p className="text-xs text-danger">{error}</p>}
      <p className="text-xs text-muted">
        {zh
          ? '导出将下载符合条件的记录。保留策略在保存后立即清除更早的记录。'
          : 'Export downloads matching records. Retention purges older records immediately on save.'}
      </p>
    </div>
  );
}

function emptyPricingEntry(): PricingEntry {
  return {
    provider: '',
    model: '',
    input_per_mtok: 0,
    output_per_mtok: 0,
    cache_read_per_mtok: 0,
    cache_create_per_mtok: 0,
    context_window: 0,
  };
}

function PricingSettings() {
  const zh = useLang().lang === 'zh';
  const [summary, setSummary] = useState<PricingSummary | null>(null);
  const [usdToCny, setUsdToCny] = useState('');
  const [overrides, setOverrides] = useState<PricingEntry[]>([]);
  const [draft, setDraft] = useState<PricingEntry>(emptyPricingEntry);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const load = () => {
    setLoading(true);
    api
      .pricingSummary()
      .then((data) => {
        setSummary(data);
        setUsdToCny(String(data.usd_to_cny));
        setOverrides(data.overrides);
      })
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    load();
  }, []);

  const handleAddOverride = () => {
    if (!draft.provider.trim() || !draft.model.trim()) return;
    setOverrides((prev) => [
      ...prev.filter((o) => !(o.provider === draft.provider && o.model === draft.model)),
      { ...draft, provider: draft.provider.trim(), model: draft.model.trim() },
    ]);
    setDraft(emptyPricingEntry());
  };

  const handleRemoveOverride = (index: number) => {
    setOverrides((prev) => prev.filter((_, i) => i !== index));
  };

  const handleSave = async () => {
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      const rate = parseFloat(usdToCny);
      const data = await api.setPricingConfig({
        usd_to_cny: Number.isFinite(rate) ? rate : null,
        overrides,
      });
      setSummary(data);
      setUsdToCny(String(data.usd_to_cny));
      setOverrides(data.overrides);
      setMessage(zh ? '定价设置已保存' : 'Pricing settings saved');
    } catch (e) {
      setError(e instanceof Error ? e.message : zh ? '保存定价失败' : 'Failed to save pricing');
    } finally {
      setSaving(false);
    }
  };

  if (loading) return <LoadingRows rows={4} />;
  if (error && !summary) return <p className="text-xs text-danger">{error}</p>;
  if (!summary) return null;

  return (
    <div className="space-y-4">
      <dl className="grid grid-cols-2 gap-3">
        <div className="surface-2 px-3 py-2.5">
          <dt className="label-xs">{zh ? '已定价模型' : 'Models priced'}</dt>
          <dd className="num mt-1 text-lg font-semibold text-heading">{summary.model_count}</dd>
        </div>
        <div className="surface-2 px-3 py-2.5">
          <dt className="label-xs">{zh ? '供应商' : 'Providers'}</dt>
          <dd className="num mt-1 text-lg font-semibold text-heading">{summary.provider_count}</dd>
        </div>
      </dl>

      <div>
        <label htmlFor="usd-cny" className="label-xs block mb-1.5">
          {zh ? 'USD → CNY 汇率' : 'USD → CNY rate'}
        </label>
        <input
          id="usd-cny"
          type="number"
          min="0"
          step="0.01"
          value={usdToCny}
          onChange={(e) => setUsdToCny(e.target.value)}
          className="input w-full"
        />
      </div>

      {overrides.length > 0 && (
        <ul className="max-h-40 overflow-y-auto space-y-1.5 pr-1">
          {overrides.map((entry, index) => (
            <li
              key={`${entry.provider}/${entry.model}`}
              className="surface-2 flex items-center justify-between gap-2 px-3 py-2 text-xs"
            >
              <span className="num truncate">
                {zh
                  ? `${entry.provider}/${entry.model} — 输入 $${entry.input_per_mtok}，输出 $${entry.output_per_mtok}`
                  : `${entry.provider}/${entry.model} — in $${entry.input_per_mtok}, out $${entry.output_per_mtok}`}
              </span>
              <button
                type="button"
                onClick={() => handleRemoveOverride(index)}
                className="shrink-0 text-xs font-medium text-danger hover:underline"
              >
                {zh ? '移除' : 'Remove'}
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className="surface-2 p-3 space-y-2">
        <p className="label-xs">
          {zh ? '添加或更新覆盖项（每百万 token，USD）' : 'Add or update override (per million tokens, USD)'}
        </p>
        <div className="grid grid-cols-2 gap-2">
          <input
            placeholder={zh ? '供应商' : 'Provider'}
            value={draft.provider}
            onChange={(e) => setDraft({ ...draft, provider: e.target.value })}
            className="input w-full"
          />
          <input
            placeholder={zh ? '模型' : 'Model'}
            value={draft.model}
            onChange={(e) => setDraft({ ...draft, model: e.target.value })}
            className="input w-full"
          />
          <input
            type="number"
            min="0"
            step="0.01"
            placeholder={zh ? '输入 $/M' : 'Input $/M'}
            value={draft.input_per_mtok || ''}
            onChange={(e) => setDraft({ ...draft, input_per_mtok: parseFloat(e.target.value) || 0 })}
            className="input w-full"
          />
          <input
            type="number"
            min="0"
            step="0.01"
            placeholder={zh ? '输出 $/M' : 'Output $/M'}
            value={draft.output_per_mtok || ''}
            onChange={(e) => setDraft({ ...draft, output_per_mtok: parseFloat(e.target.value) || 0 })}
            className="input w-full"
          />
        </div>
        <button type="button" onClick={handleAddOverride} className="btn">
          {zh ? '添加覆盖项' : 'Add override'}
        </button>
      </div>

      <button
        type="button"
        onClick={handleSave}
        disabled={saving}
        className="btn btn-primary"
      >
        {saving ? (zh ? '保存中…' : 'Saving…') : zh ? '保存定价' : 'Save pricing'}
      </button>
      {message && <p className="text-xs text-success">{message}</p>}
      {error && <p className="text-xs text-danger">{error}</p>}
    </div>
  );
}

function CollectorsSettings() {
  const zh = useLang().lang === 'zh';
  const [collectors, setCollectors] = useState<Array<{ id: string; name: string; available: boolean; enabled: boolean; last_scan_at: string | null }>>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    api
      .collectors()
      .then(setCollectors)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  }, []);

  const toggleCollector = (id: string, enabled: boolean) => {
    setCollectors((prev) => prev.map((c) => (c.id === id ? { ...c, enabled } : c)));
  };

  const handleSave = async () => {
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      const enabled: Record<string, boolean> = {};
      for (const c of collectors) {
        enabled[c.id] = c.enabled;
      }
      const updated = await api.setCollectorsConfig({ enabled });
      setCollectors(updated);
      setMessage(zh ? '采集器设置已保存' : 'Collector settings saved');
    } catch (e) {
      setError(e instanceof Error ? e.message : zh ? '保存采集器失败' : 'Failed to save collectors');
    } finally {
      setSaving(false);
    }
  };

  if (loading) return <LoadingRows rows={4} />;
  if (error && collectors.length === 0) return <p className="text-xs text-danger">{error}</p>;

  const detected = collectors.filter((c) => c.available).length;
  const enabledCount = collectors.filter((c) => c.enabled).length;

  return (
    <div className="space-y-4">
      <p className="num text-xs text-muted">
        {zh
          ? `共 ${collectors.length} 个采集器 · 已检测到 ${detected} 个 · 已启用 ${enabledCount} 个`
          : `${detected} detected · ${enabledCount} enabled of ${collectors.length} collectors`}
      </p>
      <ul className="max-h-64 overflow-y-auto space-y-1.5 pr-1">
        {collectors.map((c) => (
          <li
            key={c.id}
            className="surface-2 flex items-center justify-between gap-3 px-3 py-2 text-sm"
          >
            <label className="flex items-center gap-2 min-w-0 cursor-pointer">
              <input
                type="checkbox"
                checked={c.enabled}
                onChange={(e) => toggleCollector(c.id, e.target.checked)}
                className={CHECKBOX_CLS}
              />
              <span className="truncate">{c.name}</span>
            </label>
            <span className={`badge shrink-0 ${c.available ? 'badge-success' : ''}`}>
              {c.available ? (zh ? '已检测' : 'Detected') : zh ? '未找到' : 'Not found'}
            </span>
          </li>
        ))}
      </ul>
      <button
        type="button"
        onClick={handleSave}
        disabled={saving}
        className="btn btn-primary"
      >
        {saving ? (zh ? '保存中…' : 'Saving…') : zh ? '保存采集器' : 'Save collectors'}
      </button>
      {message && <p className="text-xs text-success">{message}</p>}
      {error && <p className="text-xs text-danger">{error}</p>}
      <p className="text-xs text-muted">
        {zh
          ? '被禁用的采集器在扫描时会被跳过。可在仪表盘运行扫描以导入用量。'
          : 'Disabled collectors are skipped during scan. Run a scan from the dashboard to ingest usage.'}
      </p>
    </div>
  );
}

const SCAN_INTERVALS = [
  { en: 'Disabled', zh: '已禁用', value: 0 },
  { en: 'Every 5 minutes', zh: '每 5 分钟', value: 5 },
  { en: 'Every 15 minutes', zh: '每 15 分钟', value: 15 },
  { en: 'Every 30 minutes', zh: '每 30 分钟', value: 30 },
  { en: 'Every 60 minutes', zh: '每 60 分钟', value: 60 },
];

function GeneralSettings() {
  const zh = useLang().lang === 'zh';
  const { theme, setTheme } = useTheme();
  const [autoScanMinutes, setAutoScanMinutes] = useState(0);
  const [launchAtStartup, setLaunchAtStartup] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .generalConfig()
      .then((c) => {
        setAutoScanMinutes(c.auto_scan_interval_minutes);
        setLaunchAtStartup(c.launch_at_startup ?? false);
      })
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  }, []);

  const handleSave = async () => {
    setSaving(true);
    setMessage(null);
    setError(null);
    try {
      await api.setGeneralConfig({
        auto_scan_interval_minutes: autoScanMinutes,
        launch_at_startup: launchAtStartup,
      });
      setMessage(zh ? '通用设置已保存' : 'General settings saved');
    } catch (e) {
      setError(e instanceof Error ? e.message : zh ? '保存设置失败' : 'Failed to save settings');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <label htmlFor="theme-select" className="label-xs block mb-1.5">
          {zh ? '主题' : 'Theme'}
        </label>
        <select
          id="theme-select"
          value={theme}
          onChange={(e) => setTheme(e.target.value as 'dark' | 'light')}
          className="input w-full"
        >
          <option value="dark">{zh ? '深色' : 'Dark'}</option>
          <option value="light">{zh ? '浅色' : 'Light'}</option>
        </select>
      </div>

      <div>
        <label htmlFor="auto-scan" className="label-xs block mb-1.5">
          {zh ? '后台自动扫描' : 'Background auto-scan'}
        </label>
        {loading ? (
          <Skeleton className="h-8 w-full" />
        ) : (
          <select
            id="auto-scan"
            value={autoScanMinutes}
            onChange={(e) => setAutoScanMinutes(Number(e.target.value))}
            className="input w-full"
          >
            {SCAN_INTERVALS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {zh ? opt.zh : opt.en}
              </option>
            ))}
          </select>
        )}
      </div>

      <label className="flex items-center gap-2 text-sm cursor-pointer">
        <input
          type="checkbox"
          checked={launchAtStartup}
          onChange={(e) => setLaunchAtStartup(e.target.checked)}
          disabled={loading}
          className={CHECKBOX_CLS}
        />
        {zh ? '开机自启动' : 'Launch at startup'}
      </label>

      <button
        type="button"
        onClick={handleSave}
        disabled={saving || loading}
        className="btn btn-primary"
      >
        {saving ? (zh ? '保存中…' : 'Saving…') : zh ? '保存通用设置' : 'Save general'}
      </button>
      {message && <p className="text-xs text-success">{message}</p>}
      {error && <p className="text-xs text-danger">{error}</p>}
      <p className="text-xs text-muted">
        {zh ? (
          <>
            主题保存在本浏览器中。自动扫描在桌面应用和{' '}
            <code className="text-heading">alltokens serve</code> 中生效。开机自启动仅桌面端可用，保存后约一分钟内生效。
          </>
        ) : (
          <>
            Theme is saved in this browser. Auto-scan applies in the desktop app and{' '}
            <code className="text-heading">alltokens serve</code>. Launch at startup is
            desktop-only and takes effect within about a minute after saving.
          </>
        )}
      </p>
    </div>
  );
}

function BudgetSettings() {
  const zh = useLang().lang === 'zh';
  const { data, loading, save } = useBudgetConfig();
  const [monthly, setMonthly] = useState('');
  const [enabled, setEnabled] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<{ text: string; ok: boolean } | null>(null);

  useEffect(() => {
    if (data) {
      setMonthly(data.monthly_usd != null ? String(data.monthly_usd) : '');
      setEnabled(data.enabled);
    }
  }, [data]);

  const handleSave = async () => {
    setSaving(true);
    setMessage(null);
    try {
      const config: BudgetConfig = {
        monthly_usd: monthly ? parseFloat(monthly) : null,
        enabled,
      };
      await save(config);
      setMessage({ text: zh ? '预算设置已保存' : 'Budget settings saved', ok: true });
    } catch {
      setMessage({ text: zh ? '保存预算设置失败' : 'Failed to save budget settings', ok: false });
    } finally {
      setSaving(false);
    }
  };

  if (loading && !data) {
    return <LoadingRows rows={3} />;
  }

  return (
    <div className="space-y-4">
      <label className="flex items-center gap-2 text-sm cursor-pointer">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => setEnabled(e.target.checked)}
          className={CHECKBOX_CLS}
        />
        {zh ? '启用月度预算提醒' : 'Enable monthly budget alerts'}
      </label>
      <div>
        <label htmlFor="budget-monthly" className="label-xs block mb-1.5">
          {zh ? '月度预算（USD）' : 'Monthly budget (USD)'}
        </label>
        <input
          id="budget-monthly"
          type="number"
          min="0"
          step="0.01"
          value={monthly}
          onChange={(e) => setMonthly(e.target.value)}
          placeholder={zh ? '例如 100' : 'e.g. 100'}
          className="input w-full"
        />
      </div>
      <button
        type="button"
        onClick={handleSave}
        disabled={saving}
        className="btn btn-primary"
      >
        {saving ? (zh ? '保存中…' : 'Saving…') : zh ? '保存预算' : 'Save budget'}
      </button>
      {message && (
        <p className={`text-xs ${message.ok ? 'text-success' : 'text-danger'}`}>
          {message.text}
        </p>
      )}
      <p className="text-xs text-muted">
        {zh
          ? '仪表盘在用量达到 80% 时显示警告，当月超出此限额时显示告警。桌面应用也会在相同阈值发送系统通知。'
          : 'Dashboard shows a warning at 80% and an alert when the current month exceeds this limit. Desktop app also sends OS notifications at the same thresholds.'}
      </p>
    </div>
  );
}

const SUBSCRIPTION_PRESETS: SubscriptionTier[] = [
  { label: 'Claude Pro', monthly_usd: 20 },
  { label: 'Claude Max', monthly_usd: 100 },
  { label: 'Codex Plus', monthly_usd: 20 },
  { label: 'Codex Pro', monthly_usd: 200 },
  { label: 'ChatGPT Plus', monthly_usd: 20 },
];

function SubscriptionSettings() {
  const zh = useLang().lang === 'zh';
  const { data, loading, save } = useSubscriptionConfig();
  const [tiers, setTiers] = useState<SubscriptionTier[]>([]);
  const [enabled, setEnabled] = useState(false);
  const [draftLabel, setDraftLabel] = useState('');
  const [draftFee, setDraftFee] = useState('');
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<{ text: string; ok: boolean } | null>(null);

  useEffect(() => {
    if (data) {
      setTiers(data.tiers);
      setEnabled(data.enabled);
    }
  }, [data]);

  const total = tiers.reduce((sum, t) => sum + (Number.isFinite(t.monthly_usd) ? t.monthly_usd : 0), 0);

  const addTier = (tier: SubscriptionTier) => {
    setTiers((prev) => [
      ...prev.filter((t) => t.label !== tier.label),
      { label: tier.label.trim(), monthly_usd: tier.monthly_usd },
    ]);
  };

  const handleAddCustom = () => {
    const label = draftLabel.trim();
    const fee = parseFloat(draftFee);
    if (!label || !Number.isFinite(fee) || fee <= 0) return;
    addTier({ label, monthly_usd: fee });
    setDraftLabel('');
    setDraftFee('');
  };

  const handleRemove = (label: string) => {
    setTiers((prev) => prev.filter((t) => t.label !== label));
  };

  const handleSave = async () => {
    setSaving(true);
    setMessage(null);
    try {
      const config: SubscriptionConfig = { tiers, enabled };
      await save(config);
      setMessage({ text: zh ? '订阅设置已保存' : 'Subscription settings saved', ok: true });
    } catch {
      setMessage({ text: zh ? '保存订阅设置失败' : 'Failed to save subscription settings', ok: false });
    } finally {
      setSaving(false);
    }
  };

  if (loading && !data) {
    return <LoadingRows rows={3} />;
  }

  return (
    <div className="space-y-4">
      <label className="flex items-center gap-2 text-sm cursor-pointer">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => setEnabled(e.target.checked)}
          className={CHECKBOX_CLS}
        />
        {zh ? '在仪表盘显示「已省 X%」回本进度条' : 'Show “Saved X%” recoup bar on dashboard'}
      </label>

      {tiers.length > 0 && (
        <ul className="space-y-1.5">
          {tiers.map((t) => (
            <li
              key={t.label}
              className="surface-2 flex items-center justify-between gap-2 px-3 py-2 text-xs"
            >
              <span className="num truncate">
                {zh ? `${t.label} — $${t.monthly_usd}/月` : `${t.label} — $${t.monthly_usd}/mo`}
              </span>
              <button
                type="button"
                onClick={() => handleRemove(t.label)}
                className="shrink-0 text-xs font-medium text-danger hover:underline"
              >
                {zh ? '移除' : 'Remove'}
              </button>
            </li>
          ))}
        </ul>
      )}

      <div>
        <p className="label-xs mb-1.5">{zh ? '快速添加' : 'Quick add'}</p>
        <div className="flex flex-wrap gap-2">
          {SUBSCRIPTION_PRESETS.map((p) => (
            <button
              key={p.label}
              type="button"
              onClick={() => addTier(p)}
              className="btn"
            >
              {p.label} ${p.monthly_usd}
            </button>
          ))}
        </div>
      </div>

      <div className="surface-2 p-3 space-y-2">
        <p className="label-xs">{zh ? '添加自定义档位（USD / 月）' : 'Add custom tier (USD / month)'}</p>
        <div className="grid grid-cols-2 gap-2">
          <input
            placeholder={zh ? '名称' : 'Label'}
            value={draftLabel}
            onChange={(e) => setDraftLabel(e.target.value)}
            className="input w-full"
          />
          <input
            type="number"
            min="0"
            step="0.01"
            placeholder={zh ? '月费 $' : 'Monthly $'}
            value={draftFee}
            onChange={(e) => setDraftFee(e.target.value)}
            className="input w-full"
          />
        </div>
        <button type="button" onClick={handleAddCustom} className="btn">
          {zh ? '添加档位' : 'Add tier'}
        </button>
      </div>

      <p className="text-xs text-muted">
        {zh ? '月度合计：' : 'Monthly total: '}
        <span className="num text-heading font-medium">${total.toFixed(2)}</span>
      </p>

      <button
        type="button"
        onClick={handleSave}
        disabled={saving}
        className="btn btn-primary"
      >
        {saving ? (zh ? '保存中…' : 'Saving…') : zh ? '保存订阅设置' : 'Save subscription'}
      </button>
      {message && (
        <p className={`text-xs ${message.ok ? 'text-success' : 'text-danger'}`}>
          {message.text}
        </p>
      )}
      <p className="text-xs text-muted">
        {zh
          ? '仪表盘的 🐑 卡片会将本月 API 等价成本与你的订阅月费总额对比，展示订阅已“回本”多少。'
          : "The dashboard 🐑 card compares this month's API-equivalent cost against your total monthly fee to show how much the subscription has already paid for itself."}
      </p>
    </div>
  );
}

function CaSettings() {
  const zh = useLang().lang === 'zh';
  const { data, loading, error, busy, install, uninstall } = useCa();

  if (loading && !data) {
    return <LoadingRows rows={3} />;
  }

  const certPresent = data?.cert_present ?? false;
  const status = data?.status ?? 'unknown';
  const platform = data?.platform ?? '';

  let badge: { text: string; cls: string };
  if (!certPresent) {
    badge = { text: zh ? '未生成' : 'Not generated', cls: 'badge' };
  } else if (status === 'installed') {
    badge = { text: zh ? '✅ 已信任' : '✅ Trusted', cls: 'badge badge-success' };
  } else if (status === 'not_installed') {
    badge = { text: zh ? '未安装' : 'Not installed', cls: 'badge badge-warn' };
  } else {
    badge = { text: zh ? '⚠️ 无法查询' : '⚠️ Unavailable', cls: 'badge' };
  }

  const hint =
    platform === 'macos'
      ? zh
        ? '安装时 macOS 可能弹出钥匙串确认对话框，请点“始终信任”。'
        : 'macOS may show a Keychain confirmation during install — choose “Always Trust”.'
      : platform === 'linux'
        ? zh
          ? 'Linux 系统信任库需 root 权限，若安装失败请在终端运行 `sudo alltokens ca install`。'
          : 'The Linux system trust store requires root. If installation fails, run `sudo alltokens ca install` in a terminal.'
        : zh
          ? '安装到当前用户的受信任根证书存储（无需管理员权限）。'
          : "Installs into the current user's trusted root certificate store (no admin rights required).";

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <span className="label-xs">{zh ? '信任库状态' : 'Trust store status'}</span>
        <span className={badge.cls}>{badge.text}</span>
      </div>

      {data?.cert_path && (
        <div className="text-xs text-faint break-all">
          {zh ? '证书路径：' : 'Certificate path: '}
          <span className="text-muted">{data.cert_path}</span>
        </div>
      )}

      <p className="text-xs text-muted">{hint}</p>

      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={install}
          disabled={busy || status === 'installed'}
          className="btn btn-primary"
        >
          {busy ? (zh ? '处理中…' : 'Working…') : zh ? '安装到信任库' : 'Install to trust store'}
        </button>
        <button
          type="button"
          onClick={uninstall}
          disabled={busy || status === 'not_installed'}
          className="btn"
        >
          {zh ? '从信任库移除' : 'Remove from trust store'}
        </button>
      </div>

      {error && (
        <p className="text-xs text-danger">
          {zh
            ? `操作失败：${error}（可改用 CLI \`alltokens ca install\`）`
            : `Operation failed: ${error} (you can use the CLI \`alltokens ca install\` instead)`}
        </p>
      )}
    </div>
  );
}

export default function Settings() {
  const zh = useLang().lang === 'zh';
  return (
    <Layout>
      <div className="space-y-4 sm:space-y-5">
        <div>
          <h2 className="text-lg font-semibold text-heading">{zh ? '设置' : 'Settings'}</h2>
          <p className="text-sm text-muted mt-1">
            {zh
              ? '配置定价、采集器与应用偏好。'
              : 'Configure pricing, collectors, and application preferences.'}
          </p>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-4 sm:gap-5 items-start">
          <Section
            title={zh ? '定价' : 'Pricing'}
            description={
              zh
                ? '管理模型定价规则与货币换算汇率。'
                : 'Manage model pricing rules and currency conversion rates.'
            }
          >
            <PricingSettings />
          </Section>
          <Section
            title={zh ? '采集器' : 'Collectors'}
            description={
              zh
                ? '启用或禁用日志采集器并配置扫描路径。'
                : 'Enable or disable log collectors and configure scan paths.'
            }
          >
            <CollectorsSettings />
          </Section>
          <Section
            title={zh ? '通用' : 'General'}
            description={
              zh
                ? '主题、语言与显示偏好。'
                : 'Theme, locale, and display preferences.'
            }
          >
            <GeneralSettings />
          </Section>
          <Section
            title={zh ? '预算' : 'Budget'}
            description={
              zh
                ? '月度支出限额与仪表盘提醒。'
                : 'Monthly spending limit and dashboard alerts.'
            }
          >
            <BudgetSettings />
          </Section>
          <Section
            title={zh ? '订阅' : 'Subscription'}
            description={
              zh
                ? '跟踪订阅档位，展示相比按量付费节省了多少。'
                : "Track subscription tiers to show how much you've saved vs pay-as-you-go."
            }
          >
            <SubscriptionSettings />
          </Section>
          <Section
            title={zh ? 'HTTPS 拦截证书 (CA)' : 'HTTPS Interception Certificate (CA)'}
            description={
              zh
                ? '安装 MITM 根证书到系统信任库，让 --mitm 代理可解密 HTTPS 抓取 usage。'
                : 'Install the MITM root certificate into the system trust store so the --mitm proxy can decrypt HTTPS traffic to capture usage.'
            }
          >
            <CaSettings />
          </Section>
          <Section
            title={zh ? '数据' : 'Data'}
            description={
              zh
                ? '导出用量数据、清理旧记录并管理存储。'
                : 'Export usage data, clear old records, and manage storage.'
            }
          >
            <DataExport />
          </Section>
        </div>
      </div>
    </Layout>
  );
}
