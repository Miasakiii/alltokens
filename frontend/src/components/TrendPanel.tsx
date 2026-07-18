import { useState } from 'react';
import type { DailySummary } from '../api/types';
import { formatTokens } from '../utils/format';
import { useLang } from '../i18n';
import SegmentedControl, { type SegmentedAccent, type SegmentedOption } from './ui/SegmentedControl';
import { Card, EmptyState, Skeleton } from './ui/primitives';

type Metric = 'tokens' | 'cost' | 'cache';
type Granularity = 'daily' | 'weekly';

interface Props {
  data: DailySummary[] | null;
  loading: boolean;
}

interface ChartPoint {
  label: string;
  tokens: number;
  cost: number;
  hitRate: number;
  cacheRead: number;
}

function weekKey(date: string): string {
  const d = new Date(`${date}T00:00:00`);
  const day = d.getDay() || 7;
  const monday = new Date(d);
  monday.setDate(d.getDate() - day + 1);
  return monday.toISOString().slice(0, 10);
}

function fmtCost(n: number): string {
  if (n >= 100) return `$${n.toFixed(0)}`;
  if (n >= 1) return `$${n.toFixed(2)}`;
  return `$${n.toFixed(4)}`;
}

/** Aggregate daily rows into chronological chart points (daily or Monday-start weeks). */
function aggregate(data: DailySummary[], granularity: Granularity): ChartPoint[] {
  const map = new Map<
    string,
    { tokens: number; cost: number; input: number; cacheRead: number; cacheCreation: number }
  >();

  for (const row of data) {
    const key = granularity === 'weekly' ? weekKey(row.date) : row.date;
    const existing = map.get(key);
    if (existing) {
      existing.tokens += row.total_tokens;
      existing.cost += row.total_cost_usd;
      existing.input += row.total_input;
      existing.cacheRead += row.total_cache_read;
      existing.cacheCreation += row.total_cache_creation;
    } else {
      map.set(key, {
        tokens: row.total_tokens,
        cost: row.total_cost_usd,
        input: row.total_input,
        cacheRead: row.total_cache_read,
        cacheCreation: row.total_cache_creation,
      });
    }
  }

  return Array.from(map.entries())
    .map(([label, v]) => {
      const cacheable = v.input + v.cacheCreation + v.cacheRead;
      return {
        label,
        tokens: v.tokens,
        cost: v.cost,
        hitRate: cacheable > 0 ? v.cacheRead / cacheable : 0,
        cacheRead: v.cacheRead,
      };
    })
    .sort((a, b) => a.label.localeCompare(b.label));
}

interface MetricDef {
  label: { zh: string; en: string };
  accent: SegmentedAccent;
  value: (p: ChartPoint) => number;
  tooltip: (p: ChartPoint, zh: boolean) => string;
  formatMax: (max: number) => string;
}

const METRIC_DEFS: Record<Metric, MetricDef> = {
  tokens: {
    label: { zh: 'Tokens', en: 'Tokens' },
    accent: 'indigo',
    value: (p) => p.tokens,
    tooltip: (p) => `${formatTokens(p.tokens)} tokens · $${p.cost.toFixed(4)}`,
    formatMax: (max) => formatTokens(max),
  },
  cost: {
    label: { zh: '成本', en: 'Cost' },
    accent: 'emerald',
    value: (p) => p.cost,
    tooltip: (p) => `${fmtCost(p.cost)} · ${formatTokens(p.tokens)} tokens`,
    formatMax: (max) => fmtCost(max),
  },
  cache: {
    label: { zh: '缓存率', en: 'Cache hit' },
    accent: 'violet',
    value: (p) => p.hitRate,
    tooltip: (p, zh) =>
      zh
        ? `${(p.hitRate * 100).toFixed(1)}% · 缓存 ${formatTokens(p.cacheRead)}`
        : `${(p.hitRate * 100).toFixed(1)}% · ${formatTokens(p.cacheRead)} cached`,
    formatMax: (max) => `${(max * 100).toFixed(1)}%`,
  },
};

/** 指标 / 粒度切换选项按当前语言生成（label 为纯展示文案）。 */
function metricOptions(zh: boolean): SegmentedOption<Metric>[] {
  return (Object.keys(METRIC_DEFS) as Metric[]).map((key) => ({
    value: key,
    label: zh ? METRIC_DEFS[key].label.zh : METRIC_DEFS[key].label.en,
  }));
}

function granularityOptions(zh: boolean): SegmentedOption<Granularity>[] {
  return [
    { value: 'daily', label: zh ? '按日' : 'Daily' },
    { value: 'weekly', label: zh ? '按周' : 'Weekly' },
  ];
}

/** 面板外层统一 Card（标题 + actions 切换器），高度与右侧 ProviderPie 协调。 */
const PANEL_CLASS = 'h-full flex flex-col';
const PANEL_BODY_CLASS = 'px-4 pb-4 flex-1 flex flex-col justify-center';

/**
 * Unified trends panel — one wide chart with metric tabs (Tokens / Cost /
 * Cache hit rate) and a daily/weekly granularity switch. Hand-drawn bars use
 * `var(--chart-1)`; the dashed average line uses `var(--app-accent)`; grid
 * lines and axis text use border/faint tokens. Tooltip uses `.theme-tooltip`.
 */
export default function TrendPanel({ data, loading }: Props) {
  const zh = useLang().lang === 'zh';
  const [metric, setMetric] = useState<Metric>('tokens');
  const [granularity, setGranularity] = useState<Granularity>('daily');

  if (loading) {
    return (
      <Card title={zh ? '趋势' : 'Trends'} className={PANEL_CLASS} bodyClassName={PANEL_BODY_CLASS}>
        <div>
          <Skeleton className="h-48 w-full" />
          <Skeleton className="mt-2 h-3 w-1/2" />
        </div>
      </Card>
    );
  }

  if (!data || data.length === 0) {
    return (
      <Card title={zh ? '趋势' : 'Trends'} className={PANEL_CLASS} bodyClassName={PANEL_BODY_CLASS}>
        <EmptyState
          title={zh ? '暂无数据' : 'No data yet'}
          hint={zh ? '记录使用活动后，这里会显示用量趋势' : 'Usage trends appear here once activity is recorded'}
          className="flex-1"
        />
      </Card>
    );
  }

  const def = METRIC_DEFS[metric];
  const chartData = aggregate(data, granularity);
  const maxValue = Math.max(...chartData.map((p) => def.value(p)), 0.0001);
  const avgValue = chartData.reduce((sum, p) => sum + def.value(p), 0) / chartData.length;
  const avgPct = Math.min(100, (avgValue / maxValue) * 100);
  const firstLabel = chartData[0].label.slice(5);
  const lastLabel = chartData[chartData.length - 1].label.slice(5);

  return (
    <Card
      title={zh ? '趋势' : 'Trends'}
      className={PANEL_CLASS}
      bodyClassName={PANEL_BODY_CLASS}
      actions={
        <div className="flex flex-wrap items-center justify-end gap-2">
          <SegmentedControl
            options={metricOptions(zh)}
            value={metric}
            onChange={setMetric}
            accent={def.accent}
          />
          <SegmentedControl
            options={granularityOptions(zh)}
            value={granularity}
            onChange={setGranularity}
            accent={def.accent}
          />
        </div>
      }
    >
      <div>
        <div className="flex h-48 gap-2">
          {/* Y 轴刻度文字（max / mid / 0） */}
          <div className="flex shrink-0 flex-col justify-between text-right">
            <span className="num text-[10px] leading-none text-faint">{def.formatMax(maxValue)}</span>
            <span className="num text-[10px] leading-none text-faint">{def.formatMax(maxValue / 2)}</span>
            <span className="num text-[10px] leading-none text-faint">0</span>
          </div>

          <div className="relative min-w-0 flex-1">
            {/* 网格线 */}
            <div
              className="absolute inset-x-0 top-0 border-t"
              style={{ borderColor: 'var(--app-surface-border)' }}
            />
            <div
              className="absolute inset-x-0 top-1/2 border-t"
              style={{ borderColor: 'var(--app-surface-border)' }}
            />
            <div
              className="absolute inset-x-0 bottom-0 border-t"
              style={{ borderColor: 'var(--app-surface-border-strong)' }}
            />

            {/* 柱 */}
            <div className="absolute inset-0 flex items-end gap-1">
              {chartData.map((p) => {
                const value = def.value(p);
                const height = (value / maxValue) * 100;
                return (
                  <div
                    key={p.label}
                    className="group flex h-full min-w-0 flex-1 flex-col justify-end"
                  >
                    <div
                      className="relative w-full rounded-t-[3px] opacity-80 transition-opacity group-hover:opacity-100"
                      style={{
                        height: `${height}%`,
                        minHeight: 4,
                        background: 'var(--chart-1)',
                      }}
                    >
                      <div className="theme-tooltip pointer-events-none absolute bottom-full left-1/2 z-10 mb-1.5 -translate-x-1/2 rounded-lg px-2.5 py-1.5 text-xs whitespace-nowrap opacity-0 transition-opacity group-hover:opacity-100">
                        <div className="num text-[10px] opacity-70">{p.label}</div>
                        <div className="num font-medium">{def.tooltip(p, zh)}</div>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>

            {/* 平均值参考线 */}
            {chartData.length > 1 && avgValue > 0 && (
              <div
                className="absolute inset-x-0"
                style={{ bottom: `${avgPct}%`, borderTop: '1px dashed var(--app-accent)' }}
              >
                <span className="num absolute -top-3.5 right-0 text-[10px] text-accent">
                  {zh ? '均值' : 'avg'} {def.formatMax(avgValue)}
                </span>
              </div>
            )}
          </div>
        </div>

        {/* X 轴范围与点数 */}
        <div className="mt-2 flex items-center justify-between text-[11px] text-faint">
          <span className="num">{firstLabel}</span>
          <span>
            {zh
              ? `${chartData.length} ${granularity === 'daily' ? '天' : '周'}`
              : `${chartData.length} ${granularity === 'daily' ? 'days' : 'weeks'}`}
          </span>
          <span className="num">{lastLabel}</span>
        </div>
      </div>
    </Card>
  );
}
