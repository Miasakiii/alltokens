import type { ProviderStats } from '../api/types';
import { formatTokens } from '../utils/format';
import { useLang } from '../i18n';
import { Card, EmptyState, LoadingRows, Skeleton } from './ui/primitives';

interface Props {
  data: ProviderStats[] | null;
  loading: boolean;
}

/** 循环使用图表分类色 var(--chart-1..8) */
function chartColor(i: number): string {
  return `var(--chart-${(i % 8) + 1})`;
}

/** 周长 ≈ 100 的圆半径，stroke-dasharray 可直接使用百分比 */
const DONUT_R = 15.9155;

/** 与左侧 TrendPanel 协调的外层 Card 结构 */
const PANEL_CLASS = 'h-full flex flex-col';
const PANEL_BODY_CLASS = 'px-4 pb-4 flex-1 flex flex-col justify-center';

/**
 * Provider share panel — hand-drawn SVG donut (segment colors cycle through
 * var(--chart-1..8)), center shows the grand total, legend lists top entries
 * with share percentages. Empty state uses EmptyState, loading uses Skeleton.
 */
export default function ProviderPie({ data, loading }: Props) {
  const zh = useLang().lang === 'zh';

  if (loading) {
    return (
      <Card
        title={zh ? '供应商分布' : 'Provider Distribution'}
        subtitle={zh ? '按总 tokens 计' : 'By total tokens'}
        className={PANEL_CLASS}
        bodyClassName={PANEL_BODY_CLASS}
      >
        <div className="flex flex-col items-center gap-5 sm:flex-row">
          <Skeleton className="h-36 w-36 shrink-0 rounded-full" />
          <LoadingRows rows={5} className="w-full min-w-0 flex-1" />
        </div>
      </Card>
    );
  }

  const total = (data ?? []).reduce((s, d) => s + d.total_tokens, 0);

  if (!data || data.length === 0 || total <= 0) {
    return (
      <Card
        title={zh ? '供应商分布' : 'Provider Distribution'}
        subtitle={zh ? '按总 tokens 计' : 'By total tokens'}
        className={PANEL_CLASS}
        bodyClassName={PANEL_BODY_CLASS}
      >
        <EmptyState
          title={zh ? '暂无数据' : 'No data yet'}
          hint={zh ? '记录用量后，这里会显示各供应商占比' : 'Provider share appears here once usage is recorded'}
          className="flex-1"
        />
      </Card>
    );
  }

  let offset = 0;
  const segments = data.map((d, i) => {
    const pct = (d.total_tokens / total) * 100;
    const seg = {
      key: `${d.provider}-${i}`,
      provider: d.provider,
      tokens: d.total_tokens,
      pct,
      offset,
      color: chartColor(i),
    };
    offset += pct;
    return seg;
  });

  const visible = segments.slice(0, 8);
  const hiddenCount = segments.length - visible.length;

  return (
    <Card
      title={zh ? '供应商分布' : 'Provider Distribution'}
      subtitle={zh ? '按总 tokens 计' : 'By total tokens'}
      className={PANEL_CLASS}
      bodyClassName={PANEL_BODY_CLASS}
    >
      <div className="flex flex-col items-center gap-5 sm:flex-row">
        {/* Donut */}
        <div className="relative h-36 w-36 shrink-0">
          <svg viewBox="0 0 42 42" className="h-full w-full -rotate-90" role="img">
            <circle
              cx="21"
              cy="21"
              r={DONUT_R}
              fill="none"
              strokeWidth="5"
              style={{ stroke: 'var(--app-surface-2)' }}
            />
            {segments.map((s) =>
              s.pct > 0 ? (
                <circle
                  key={s.key}
                  cx="21"
                  cy="21"
                  r={DONUT_R}
                  fill="none"
                  strokeWidth="5"
                  strokeDasharray={`${s.pct} ${100 - s.pct}`}
                  strokeDashoffset={-s.offset}
                  style={{ stroke: s.color }}
                >
                  <title>{`${s.provider}: ${formatTokens(s.tokens)} (${s.pct.toFixed(1)}%)`}</title>
                </circle>
              ) : null,
            )}
          </svg>
          {/* 中心总计 */}
          <div className="absolute inset-0 flex flex-col items-center justify-center">
            <span className="num text-lg font-semibold text-heading">{formatTokens(total)}</span>
            <span className="text-[10px] uppercase tracking-wide text-faint">{zh ? '总计' : 'total'}</span>
          </div>
        </div>

        {/* 图例 */}
        <div className="w-full min-w-0 flex-1 space-y-2">
          {visible.map((s) => (
            <div
              key={s.key}
              className="flex min-w-0 items-center gap-2 text-xs"
              title={`${s.provider}: ${formatTokens(s.tokens)}`}
            >
              <span
                className="h-2.5 w-2.5 shrink-0 rounded-[4px]"
                style={{ background: s.color }}
              />
              <span className="truncate text-muted">{s.provider}</span>
              <span className="num ml-auto shrink-0 text-faint">{s.pct.toFixed(1)}%</span>
            </div>
          ))}
          {hiddenCount > 0 && (
            <div className="text-xs text-faint">
              {zh ? `+${hiddenCount} 更多` : `+${hiddenCount} more`}
            </div>
          )}
        </div>
      </div>
    </Card>
  );
}
