import type { CSSProperties } from 'react';
import type { HeatmapDay, TokenHeatmap } from '../api/types';
import { formatTokens } from '../utils/format';
import { Card, EmptyState, Skeleton } from './ui/primitives';
import { useLang } from '../i18n';

interface Props {
  data: TokenHeatmap | null;
  loading: boolean;
}

function parseDate(date: string): Date {
  const [y, m, d] = date.split('-').map(Number);
  return new Date(y, m - 1, d);
}

/** 分位数等分色阶：把非零值按数量均分为 4 档（q1 < q2 < q3）。
 *  数据严重右偏时（少数几十亿 token 的峰值日 + 大量小值日），用
 *  max / 分位数上限的线性比值仍会把小值全部压成 level-1 几乎不可见；
 *  分位数等分保证每档都有相当数量的格子，色阶分布均衡。 */
function computeQuantiles(values: number[]): [number, number, number] {
  const nonzero = values.filter((v) => v > 0).sort((a, b) => a - b);
  if (nonzero.length === 0) return [1, 1, 1];
  const q = (p: number) => {
    const idx = Math.min(nonzero.length - 1, Math.max(0, Math.floor(nonzero.length * p)));
    return Math.max(nonzero[idx], 1);
  };
  return [q(0.25), q(0.5), q(0.75)];
}

function intensityLevel(tokens: number, quantiles: [number, number, number]): number {
  if (tokens === 0) return 0;
  if (tokens >= quantiles[2]) return 4;
  if (tokens >= quantiles[1]) return 3;
  if (tokens >= quantiles[0]) return 2;
  return 1;
}

/** Accent 色阶（level 1-4）：浅色主题为深琥珀、暗色主题为亮琥珀。
 *  用主题变量的 rgba 显式值而非 color-mix（兼容旧浏览器），
 *  故最低档也从 40% 起步，保证光/暗主题都有可辨识的对比度。 */
function cellStyle(level: number): CSSProperties {
  if (level === 0) return { background: 'var(--app-surface-2)' };
  return { background: `var(--heatmap-accent-${level})` };
}

const CELL_CLASS =
  'w-full aspect-square rounded-sm border border-[var(--app-surface-border)] group relative';

const WEEKDAY_LABELS_EN = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
/** 周日开头的单字标签（与 getDay() 索引一致），适配 11px 窄行。 */
const WEEKDAY_LABELS_ZH = ['日', '一', '二', '三', '四', '五', '六'];

interface WeekColumn {
  weekStart: string;
  cells: (HeatmapDay | null)[];
}

function buildWeekColumns(days: HeatmapDay[]): WeekColumn[] {
  if (days.length === 0) return [];

  const first = parseDate(days[0].date);
  const leading = first.getDay();
  const padded: (HeatmapDay | null)[] = [
    ...Array.from({ length: leading }, () => null),
    ...days,
  ];

  const columns: WeekColumn[] = [];
  for (let i = 0; i < padded.length; i += 7) {
    const slice = padded.slice(i, i + 7);
    while (slice.length < 7) slice.push(null);
    const firstDay = slice.find((c) => c !== null);
    columns.push({
      weekStart: firstDay?.date ?? days[Math.min(i, days.length - 1)].date,
      cells: slice,
    });
  }
  return columns;
}

function monthLabels(columns: WeekColumn[], locale: string): { label: string; index: number }[] {
  const labels: { label: string; index: number }[] = [];
  let lastMonth = -1;
  columns.forEach((col, index) => {
    const day = col.cells.find((c) => c !== null);
    if (!day) return;
    const month = parseDate(day.date).getMonth();
    if (month !== lastMonth) {
      labels.push({
        label: parseDate(day.date).toLocaleString(locale, { month: 'short' }),
        index,
      });
      lastMonth = month;
    }
  });
  return labels;
}

export default function TokenHeatmap({ data, loading }: Props) {
  const zh = useLang().lang === 'zh';
  const weekdayLabels = zh ? WEEKDAY_LABELS_ZH : WEEKDAY_LABELS_EN;

  if (loading) {
    return (
      <Card title={zh ? 'Token 热力图' : 'Token Heatmap'}>
        <Skeleton className="h-36 w-full" />
      </Card>
    );
  }

  if (!data || data.days.length === 0) {
    return (
      <Card title={zh ? 'Token 热力图' : 'Token Heatmap'}>
        <EmptyState
          title={zh ? '暂无数据' : 'No data yet'}
          hint={zh ? '记录到每日 Token 活动后将在此显示' : 'Daily token activity will show up here once recorded'}
        />
      </Card>
    );
  }

  const quantiles = computeQuantiles(data.days.map((d) => d.total_tokens));
  const columns = buildWeekColumns(data.days);
  const months = monthLabels(columns, zh ? 'zh-CN' : 'en-US');
  const activeDays = data.days.filter((d) => d.total_tokens > 0).length;

  return (
    <Card
      title={zh ? 'Token 热力图' : 'Token Heatmap'}
      actions={
        <span className="num text-xs text-faint shrink-0">
          {zh
            ? `${data.period_days} 天 · 活跃 ${activeDays} 天`
            : `${data.period_days}d · ${activeDays} active`}
        </span>
      }
    >
      <div className="overflow-x-auto">
        <div className="inline-flex w-full min-w-[720px] flex-col gap-1">
          <div className="relative ml-10 h-4 w-[calc(100%-2.5rem)] text-[10px] text-faint">
            {months.map((m) => (
              <span
                key={`${m.label}-${m.index}`}
                className="absolute top-0 whitespace-nowrap"
                style={{ left: `${(m.index / columns.length) * 100}%` }}
              >
                {m.label}
              </span>
            ))}
          </div>

          <div className="flex min-w-0 flex-1 gap-2">
            <div className="flex w-8 shrink-0 flex-col gap-[3px] pr-1 text-[10px] text-faint">
              {weekdayLabels.map((label, i) => (
                <span
                  key={label}
                  className={`flex-1 leading-none ${i % 2 === 0 ? 'opacity-0 sm:opacity-100' : 'opacity-0'}`}
                >
                  {label}
                </span>
              ))}
            </div>

            <div className="flex min-w-0 flex-1 gap-[3px]">
              {columns.map((col, colIdx) => (
                <div key={col.weekStart + colIdx} className="flex min-w-0 flex-1 flex-col gap-[3px]">
                  {col.cells.map((cell, rowIdx) => {
                    if (!cell) {
                      return <div key={`empty-${colIdx}-${rowIdx}`} className="aspect-square w-full" />;
                    }
                    const level = intensityLevel(cell.total_tokens, quantiles);
                    return (
                      <div
                        key={cell.date}
                        className={CELL_CLASS}
                        style={cellStyle(level)}
                      >
                        {/* 首行格子向上弹 tooltip 会被 overflow-x-auto 裁掉，翻转到下方显示 */}
                        <div
                          className={`absolute left-1/2 -translate-x-1/2 theme-tooltip text-xs px-2 py-1 rounded-md opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-20 ${
                            rowIdx === 0 ? 'top-full mt-1.5' : 'bottom-full mb-1.5'
                          }`}
                        >
                          {cell.date}
                          <br />
                          {formatTokens(cell.total_tokens)} tokens · ${cell.total_cost_usd.toFixed(4)}
                        </div>
                      </div>
                    );
                  })}
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>

      <div className="flex items-center justify-end gap-1.5 mt-3 text-[10px] text-faint">
        <span>{zh ? '少' : 'Less'}</span>
        {[0, 1, 2, 3, 4].map((level) => (
          <div
            key={level}
            className="w-[11px] h-[11px] rounded-sm border border-[var(--app-surface-border)]"
            style={cellStyle(level)}
          />
        ))}
        <span>{zh ? '多' : 'More'}</span>
      </div>
    </Card>
  );
}
