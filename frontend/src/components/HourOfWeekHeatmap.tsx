import type { CSSProperties } from 'react';
import type { HourOfWeekCell } from '../api/types';
import { formatTokens } from '../utils/format';
import { Card, EmptyState, Skeleton } from './ui/primitives';
import { useLang } from '../i18n';

interface Props {
  data: HourOfWeekCell[] | null;
  loading: boolean;
}

const WEEKDAY_LABELS_EN = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
/** 周日开头（与 weekday 索引 0=Sunday 一致）。 */
const WEEKDAY_LABELS_ZH = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'];
const HOURS = Array.from({ length: 24 }, (_, i) => i);

/**
 * Success 色阶（level 1-4），与 TokenHeatmap 的 accent 色阶区分。
 * 用主题变量的 rgba 显式值而非 color-mix（兼容旧浏览器），
 * 最低档也从 40% 起步，保证光/暗主题都有可辨识的对比度。
 * level 0 为空格（surface-2）。
 */
function cellStyle(level: number): CSSProperties {
  if (level === 0) return { background: 'var(--app-surface-2)' };
  return { background: `var(--heatmap-success-${level})` };
}

const CELL_CLASS =
  'flex-1 min-w-0 aspect-square rounded-sm border border-[var(--app-surface-border)] group relative';

/** 分位数等分色阶：把非零值按数量均分为 4 档（q1 < q2 < q3）。
 *  数据严重右偏时（少数极端时段 + 大量小值时段），用 max / 分位数上限的
 *  线性比值仍会把小值全部压成 level-1 几乎不可见；分位数等分保证每档
 *  都有相当数量的格子，色阶分布均衡。 */
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

/** `UTC+8` / `UTC-5` style label for the browser (= server) timezone. */
function localTzLabel(): string {
  const offsetHours = -new Date().getTimezoneOffset() / 60;
  const sign = offsetHours >= 0 ? '+' : '-';
  const abs = Math.abs(offsetHours);
  return `UTC${sign}${Number.isInteger(abs) ? abs : abs.toFixed(1)}`;
}

/**
 * Weekday x hour activity grid (agentsview-style). Unlike the UTC-grouped
 * daily heatmap, cells are aggregated in server-local time — see
 * `Storage::get_hour_of_week`.
 */
export default function HourOfWeekHeatmap({ data, loading }: Props) {
  const zh = useLang().lang === 'zh';
  const weekdayLabels = zh ? WEEKDAY_LABELS_ZH : WEEKDAY_LABELS_EN;

  if (loading) {
    return (
      <Card title={zh ? '星期 × 小时活跃度' : 'Activity by Day and Hour'}>
        <Skeleton className="h-36 w-full" />
      </Card>
    );
  }

  if (!data || data.length === 0) {
    return (
      <Card title={zh ? '星期 × 小时活跃度' : 'Activity by Day and Hour'}>
        <EmptyState
          title={zh ? '暂无数据' : 'No data yet'}
          hint={zh ? '记录到分时段活动后将在此显示' : 'Hourly activity will show up here once recorded'}
        />
      </Card>
    );
  }

  const lookup = new Map<string, HourOfWeekCell>();
  for (const c of data) lookup.set(`${c.weekday}-${c.hour}`, c);
  const quantiles = computeQuantiles(data.map((c) => c.total_tokens));
  const activeSlots = data.filter((c) => c.total_tokens > 0).length;

  return (
    <Card
      title={zh ? '星期 × 小时活跃度' : 'Activity by Day and Hour'}
      actions={
        <span className="num text-xs text-faint shrink-0">
          {zh
            ? `${localTzLabel()} · ${activeSlots} 个活跃时段`
            : `${localTzLabel()} · ${activeSlots} active slots`}
        </span>
      }
    >
      <div className="overflow-x-auto">
        <div className="inline-flex w-full min-w-[720px] flex-col gap-[3px]">
          <div className="flex h-4 gap-[3px] pl-9 text-[10px] text-faint">
            {HOURS.map((h) => (
              <span key={h} className="flex-1 text-center leading-4">
                {h % 3 === 0 ? h : ''}
              </span>
            ))}
          </div>

          {weekdayLabels.map((label, weekday) => (
            <div key={label} className="flex min-w-0 flex-1 items-center gap-[3px]">
              <span className="w-8 shrink-0 pr-1 text-right text-[10px] text-faint">{label}</span>
              {HOURS.map((hour) => {
                const cell = lookup.get(`${weekday}-${hour}`);
                const tokens = cell?.total_tokens ?? 0;
                const level = intensityLevel(tokens, quantiles);
                return (
                  <div
                    key={hour}
                    className={CELL_CLASS}
                    style={cellStyle(level)}
                  >
                    {/* 首行(周日)向上弹 tooltip 会被 overflow-x-auto 裁掉，翻转到下方显示 */}
                    <div
                      className={`absolute left-1/2 -translate-x-1/2 theme-tooltip text-xs px-2 py-1 rounded-md opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-20 ${
                        weekday === 0 ? 'top-full mt-1.5' : 'bottom-full mb-1.5'
                      }`}
                    >
                      {label} {hour}:00
                      <br />
                      {formatTokens(tokens)} tokens ·{' '}
                      {zh
                        ? `${formatTokens(cell?.request_count ?? 0)} 次请求`
                        : `${formatTokens(cell?.request_count ?? 0)} reqs`}
                    </div>
                  </div>
                );
              })}
            </div>
          ))}
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
