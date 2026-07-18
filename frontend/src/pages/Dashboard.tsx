import { useEffect, useMemo, useState } from 'react';
import Layout from '../components/Layout';
import StatsCards from '../components/StatsCards';
import TodaySummary from '../components/TodaySummary';
import TrendPanel from '../components/TrendPanel';
import BudgetAlert from '../components/BudgetAlert';
import CodexQuotaCard from '../components/CodexQuotaCard';
import ClaudeQuotaCard from '../components/ClaudeQuotaCard';
import ProviderPie from '../components/ProviderPie';
import ModelBreakdown from '../components/ModelBreakdown';
import ToolBreakdown from '../components/ToolBreakdown';
import ToolInvocationBreakdown from '../components/ToolInvocationBreakdown';
import SkillInvocationBreakdown from '../components/SkillInvocationBreakdown';
import ProjectBreakdown from '../components/ProjectBreakdown';
import SessionBreakdown from '../components/SessionBreakdown';
import TokenHeatmap from '../components/TokenHeatmap';
import HourOfWeekHeatmap from '../components/HourOfWeekHeatmap';
import StatusBar from '../components/StatusBar';
import FreshnessLabel from '../components/ui/FreshnessLabel';
import SegmentedControl from '../components/ui/SegmentedControl';
import RequestFilters, { type RequestFilterValues } from '../components/RequestFilters';
import RequestTable from '../components/RequestTable';
import { api } from '../api/client';
import { useOverview, useProviders, useModels, useTools, useToolsRanking, useSkillsRanking, useProjects, useSessions, useTrends, useHeatmap, useHourOfWeek, useRequests } from '../hooks/useStats';
import { useBudgetConfig } from '../hooks/useBudget';
import { useSubscriptionConfig } from '../hooks/useSubscription';
import { todayStartISO, weekStartISO, monthStartISO } from '../utils/dates';
import { useCodexQuota } from '../hooks/useCodexQuota';
import { useClaudeQuota } from '../hooks/useClaudeQuota';
import { usePricingModels } from '../hooks/usePricingModels';
import { useScanComplete } from '../hooks/useWebSocket';
import { useLang } from '../i18n';

const periodOptions = (zh: boolean) => [
  { label: zh ? '今天' : 'Today', value: '0d' },
  { label: zh ? '近 7 天' : '7 Days', value: '7d' },
  { label: zh ? '近 30 天' : '30 Days', value: '30d' },
  { label: zh ? '近 90 天' : '90 Days', value: '90d' },
];

const EMPTY_FILTERS: RequestFilterValues = {
  provider: '',
  model: '',
  collector: '',
  tool: '',
};

export default function Dashboard() {
  const zh = useLang().lang === 'zh';
  const [period, setPeriod] = useState('7d');
  const [filters, setFilters] = useState<RequestFilterValues>(EMPTY_FILTERS);
  const [page, setPage] = useState(0);
  const [scanning, setScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [lastUpdated, setLastUpdated] = useState<number | null>(null);

  const periods = useMemo(() => periodOptions(zh), [zh]);

  // Concrete date range for the selected period, shown next to the switcher.
  const rangeLabel = useMemo(() => {
    const now = new Date();
    const end = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const zhDay = (d: Date, withYear: boolean) =>
      `${withYear ? `${d.getFullYear()}年` : ''}${d.getMonth() + 1}月${d.getDate()}日`;
    const enDay = (d: Date, withYear: boolean) =>
      d.toLocaleDateString(
        'en-US',
        withYear
          ? { month: 'short', day: 'numeric', year: 'numeric' }
          : { month: 'short', day: 'numeric' },
      );
    if (period === '0d') {
      return zh ? `今天 ${zhDay(end, false)}` : `Today, ${enDay(end, false)}`;
    }
    const days = parseInt(period, 10); // 7 / 30 / 90
    const start = new Date(end);
    start.setDate(start.getDate() - (days - 1));
    const crossYear = start.getFullYear() !== end.getFullYear();
    return zh
      ? `${zhDay(start, crossYear)} – ${zhDay(end, crossYear)}`
      : `${enDay(start, crossYear)} – ${enDay(end, crossYear)}`;
  }, [period, zh]);

  const query = { last: period === '0d' ? undefined : period };
  const filterQuery = {
    ...query,
    provider: filters.provider || undefined,
    model: filters.model || undefined,
    collector: filters.collector || undefined,
    tool: filters.tool || undefined,
  };

  const overview = useOverview(filterQuery);
  const todayOverview = useOverview({ start_date: todayStartISO() });
  const weekOverview = useOverview({ start_date: weekStartISO() });
  const monthlyOverview = useOverview({ start_date: monthStartISO() });
  const budget = useBudgetConfig();
  const subscription = useSubscriptionConfig();
  const codexQuota = useCodexQuota(true);
  const claudeQuota = useClaudeQuota(true);
  const { contextWindowFor } = usePricingModels();
  const providers = useProviders(query);
  const models = useModels(query);
  const tools = useTools(query);
  const toolsRanking = useToolsRanking(query);
  const skillsRanking = useSkillsRanking(query);
  const projects = useProjects(query);
  const sessions = useSessions(filterQuery);
  const trends = useTrends(filterQuery);
  const heatmap = useHeatmap(filterQuery);
  const hourOfWeek = useHourOfWeek(filterQuery);
  const requests = useRequests({ ...filterQuery, page, page_size: 20 });

  const providerNames = providers.data?.map((p) => p.provider) ?? [];
  const modelNames = models.data?.map((m) => m.model) ?? [];

  const handleFiltersChange = (next: RequestFilterValues) => {
    setFilters(next);
    setPage(0);
  };

  const refresh = () => {
    overview.refetch();
    todayOverview.refetch();
    weekOverview.refetch();
    providers.refetch();
    models.refetch();
    tools.refetch();
    toolsRanking.refetch();
    skillsRanking.refetch();
    projects.refetch();
    sessions.refetch();
    trends.refetch();
    heatmap.refetch();
    hourOfWeek.refetch();
    requests.refetch();
    monthlyOverview.refetch();
    budget.refetch();
    subscription.refetch();
    codexQuota.refetch(true);
    claudeQuota.refetch(true);
  };

  // Freshness timestamp follows the main overview query (fired on mount,
  // period/filter change, manual refresh, and scan-complete pushes).
  useEffect(() => {
    if (overview.data) setLastUpdated(Date.now());
  }, [overview.data]);

  const { connected } = useScanComplete(refresh);

  const handleScan = async () => {
    setScanning(true);
    setScanError(null);
    try {
      await api.scan();
      refresh();
    } catch (e) {
      setScanError(e instanceof Error ? e.message : zh ? '扫描失败' : 'Scan failed');
    } finally {
      setScanning(false);
    }
  };

  const headerActions = (
    <div className="flex items-center gap-2 max-w-full overflow-x-auto">
      <div className="shrink-0">
        <SegmentedControl options={periods} value={period} onChange={setPeriod} />
      </div>
      <span className="text-faint text-xs num whitespace-nowrap shrink-0">{rangeLabel}</span>
      <span className="text-xs text-faint whitespace-nowrap shrink-0 hidden md:inline">
        <FreshnessLabel lastUpdated={lastUpdated} />
      </span>
      {scanError && (
        <span className="text-xs text-danger whitespace-nowrap shrink-0" title={scanError}>
          {zh ? '扫描失败' : 'Scan failed'}
        </span>
      )}
      <button
        type="button"
        onClick={handleScan}
        disabled={scanning}
        className="btn shrink-0"
        title={zh ? '立即扫描所有采集器' : 'Scan all collectors now'}
      >
        {scanning ? (
          <svg className="w-3.5 h-3.5 animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M21 12a9 9 0 1 1-6.219-8.56" />
          </svg>
        ) : (
          <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="2" />
            <path d="M16.24 7.76a6 6 0 0 1 0 8.49" />
            <path d="M7.76 16.24a6 6 0 0 1 0-8.49" />
            <path d="M19.07 4.93a10 10 0 0 1 0 14.14" />
            <path d="M4.93 19.07a10 10 0 0 1 0-14.14" />
          </svg>
        )}
        <span>{scanning ? (zh ? '扫描中…' : 'Scanning…') : zh ? '扫描' : 'Scan'}</span>
      </button>
      <button
        type="button"
        onClick={refresh}
        className="icon-btn shrink-0"
        title={zh ? '刷新' : 'Refresh'}
      >
        <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <polyline points="23 4 23 10 17 10" />
          <polyline points="1 20 1 14 7 14" />
          <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
        </svg>
      </button>
    </div>
  );

  return (
    <Layout
      actions={headerActions}
      footer={<StatusBar stats={overview.data} connected={connected} lastUpdated={lastUpdated} />}
    >
      {/* Summary — today / week / month + subscription */}
      <TodaySummary
        today={{ stats: todayOverview.data, loading: todayOverview.loading }}
        week={{ stats: weekOverview.data, loading: weekOverview.loading }}
        month={{ stats: monthlyOverview.data, loading: monthlyOverview.loading }}
        subscription={{
          feeTotal: (subscription.data?.tiers ?? []).reduce((sum, t) => sum + t.monthly_usd, 0),
          enabled: subscription.data?.enabled ?? false,
        }}
      />

      <BudgetAlert
        config={budget.data}
        monthlyCostUsd={monthlyOverview.data?.total_cost_usd ?? 0}
        loading={budget.loading || monthlyOverview.loading}
      />

      {/* Quota — plan usage for Codex / Claude */}
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
        <CodexQuotaCard
          snapshot={codexQuota.data?.snapshot}
          error={codexQuota.data?.error}
          loading={codexQuota.loading}
        />
        <ClaudeQuotaCard
          snapshot={claudeQuota.data?.snapshot}
          error={claudeQuota.data?.error}
          loading={claudeQuota.loading}
        />
      </div>

      {/* Overview metrics */}
      <StatsCards stats={overview.data} loading={overview.loading} />

      {/* Heatmaps — daily tokens + hour-of-week rhythm */}
      <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">
        <TokenHeatmap data={heatmap.data} loading={heatmap.loading} />
        <HourOfWeekHeatmap data={hourOfWeek.data} loading={hourOfWeek.loading} />
      </div>

      {/* Trend + provider share */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4 min-w-0">
        <div className="lg:col-span-2 min-w-0">
          <TrendPanel data={trends.data} loading={trends.loading} />
        </div>
        <div className="min-w-0">
          <ProviderPie data={providers.data} loading={providers.loading} />
        </div>
      </div>

      {/* Distribution — model / tool / project */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
        <ModelBreakdown data={models.data} loading={models.loading} />
        <ToolBreakdown data={tools.data} loading={tools.loading} />
        <ProjectBreakdown data={projects.data} loading={projects.loading} />
      </div>

      {/* Invocation rankings — tools / skills */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <ToolInvocationBreakdown data={toolsRanking.data} loading={toolsRanking.loading} />
        <SkillInvocationBreakdown data={skillsRanking.data} loading={skillsRanking.loading} />
      </div>

      {/* Sessions */}
      <SessionBreakdown data={sessions.data} loading={sessions.loading} />

      {/* Request details — filters + table */}
      <div className="space-y-4 min-w-0">
        <RequestFilters
          values={filters}
          providers={providerNames}
          models={modelNames}
          onChange={handleFiltersChange}
        />

        <RequestTable
          data={requests.data?.items ?? null}
          loading={requests.loading}
          page={requests.data?.page ?? 0}
          pageSize={requests.data?.page_size ?? 20}
          total={requests.data?.total ?? 0}
          onPageChange={setPage}
          contextWindowFor={contextWindowFor}
        />
      </div>
    </Layout>
  );
}
