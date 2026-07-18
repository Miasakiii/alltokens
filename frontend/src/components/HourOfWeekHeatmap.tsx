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
 * Success 色透明度梯度（原 emerald 级阶的 token 化），
 * 与 TokenHeatmap 的 accent 梯度区分。level 0 为空格（surface-2）。
 */
const LEVEL_OPACITY: Record<number, number> = { 1: 0.25, 2: 0.45, 3: 0.65, 4: 0.9 };

function cellStyle(level: number): CSSProperties {
  if (level === 0) return { background: 'var(--app-surface-2)' };
  return {
    background: `color-mix(in srgb, var(--app-success) ${LEVEL_OPACITY[level] * 100}%, transparent)`,
  };
}

const CELL_CLASS =
  'w-[14px] h-[14px] rounded-sm border border-[var(--app-surface-border)] group relative';

function intensityLevel(tokens: number, max: number): number {
  if (tokens === 0 || max === 0) return 0;
  const ratio = tokens / max;
  if (ratio >= 0.75) return 4;
  if (ratio >= 0.5) return 3;
  if (ratio >= 0.25) return 2;
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
  const maxTokens = Math.max(...data.map((c) => c.total_tokens), 1);
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
        <div className="inline-flex flex-col gap-[3px] min-w-0">
          <div className="flex gap-[3px] pl-9 text-[10px] text-faint h-4">
            {HOURS.map((h) => (
              <span key={h} className="w-[14px] shrink-0 text-center leading-4">
                {h % 3 === 0 ? h : ''}
              </span>
            ))}
          </div>

          {weekdayLabels.map((label, weekday) => (
            <div key={label} className="flex gap-[3px] items-center">
              <span className="w-8 pr-1 text-right text-[10px] text-faint shrink-0">{label}</span>
              {HOURS.map((hour) => {
                const cell = lookup.get(`${weekday}-${hour}`);
                const tokens = cell?.total_tokens ?? 0;
                const level = intensityLevel(tokens, maxTokens);
                return (
                  <div
                    key={hour}
                    className={CELL_CLASS}
                    style={cellStyle(level)}
                  >
                    <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 theme-tooltip text-xs px-2 py-1 rounded-md opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-20">
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
