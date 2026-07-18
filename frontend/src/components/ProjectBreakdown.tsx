import { useState } from 'react';
import type { ProjectStats } from '../api/types';
import { formatTokens } from '../utils/format';
import { useLang } from '../i18n';
import { Card, LoadingRows, EmptyState, Meter } from './ui/primitives';

interface Props {
  data: ProjectStats[] | null;
  loading: boolean;
}

const TOP_N = 8;

type ChartIndex = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;
/** 按行序 1-8 循环取图表分类色 */
const chartIndex = (i: number): ChartIndex => ((i % 8) + 1) as ChartIndex;

export default function ProjectBreakdown({ data, loading }: Props) {
  const zh = useLang().lang === 'zh';
  const [expanded, setExpanded] = useState(false);

  if (loading) {
    return (
      <Card title={zh ? '项目排行' : 'Project Breakdown'}>
        <LoadingRows rows={5} />
      </Card>
    );
  }

  if (!data || data.length === 0) {
    return (
      <Card title={zh ? '项目排行' : 'Project Breakdown'}>
        <EmptyState title={zh ? '暂无项目数据' : 'No project data yet'} />
      </Card>
    );
  }

  const sorted = [...data].sort((a, b) => b.total_tokens - a.total_tokens);
  const total = sorted.reduce((s, d) => s + d.total_tokens, 0);
  const visible = expanded ? sorted : sorted.slice(0, TOP_N);
  const hiddenCount = sorted.length - TOP_N;

  return (
    <Card
      title={zh ? '项目排行' : 'Project Breakdown'}
      subtitle={
        zh
          ? `${sorted.length} 个项目 · ${formatTokens(total)} tokens`
          : `${sorted.length} projects · ${formatTokens(total)} tokens`
      }
    >
      <div className="space-y-3">
        {visible.map((d, i) => {
          const pct = total > 0 ? (d.total_tokens / total) * 100 : 0;
          const ci = chartIndex(i);
          return (
            <div key={d.project} className="min-w-0">
              <div className="flex items-baseline justify-between gap-3 mb-1.5">
                <div className="flex items-center gap-2 min-w-0">
                  <span
                    className="w-2 h-2 rounded-sm shrink-0"
                    style={{ background: `var(--chart-${ci})` }}
                  />
                  <span className="text-xs text-heading truncate" title={d.project}>
                    {d.project}
                  </span>
                </div>
                <div className="flex items-baseline gap-2 shrink-0">
                  <span
                    className="num text-xs text-heading"
                    title={`${formatTokens(d.total_tokens)} tokens · $${d.total_cost_usd.toFixed(4)}`}
                  >
                    {formatTokens(d.total_tokens)}
                  </span>
                  <span className="num text-xs text-faint w-9 text-right">{pct.toFixed(0)}%</span>
                </div>
              </div>
              <Meter ratio={total > 0 ? d.total_tokens / total : 0} chartIndex={ci} />
            </div>
          );
        })}
      </div>

      {hiddenCount > 0 && (
        <button
          type="button"
          className="btn w-full justify-center mt-4"
          onClick={() => setExpanded((v) => !v)}
        >
          {expanded
            ? zh
              ? '收起'
              : 'Show less'
            : zh
              ? `展开全部 ${sorted.length} 个项目 (+${hiddenCount})`
              : `Show all ${sorted.length} projects (+${hiddenCount})`}
        </button>
      )}
    </Card>
  );
}
