import type { SkillInvocationStats } from '../api/types';
import { formatTokens } from '../utils/format';
import { useLang } from '../i18n';
import { Card, LoadingRows, EmptyState, Meter } from './ui/primitives';

interface Props {
  data: SkillInvocationStats[] | null;
  loading: boolean;
}

export default function SkillInvocationBreakdown({ data, loading }: Props) {
  const zh = useLang().lang === 'zh';

  if (loading || !data || data.length === 0) {
    return (
      <Card
        title={zh ? 'Skill TOP' : 'Top Skills'}
        subtitle={zh ? '按调用次数排序' : 'Sorted by invocation count'}
      >
        {loading ? (
          <LoadingRows rows={5} />
        ) : (
          <EmptyState title={zh ? '暂无 Skill 调用数据' : 'No skill invocation data yet'} />
        )}
      </Card>
    );
  }

  const sorted = [...data].sort((a, b) => b.invocation_count - a.invocation_count);
  const top = sorted.slice(0, 10);
  const total = sorted.reduce((s, d) => s + d.invocation_count, 0);
  const maxCount = top[0]?.invocation_count ?? 1;

  return (
    <Card
      title={zh ? 'Skill TOP' : 'Top Skills'}
      subtitle={zh ? '按调用次数排序' : 'Sorted by invocation count'}
    >
      <div className="space-y-3">
        {top.map((d, i) => {
          const pct = total > 0 ? (d.invocation_count / total) * 100 : 0;
          const chartIndex = ((i % 8) + 1) as 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;
          return (
            <div key={d.name} className="min-w-0">
              <div className="flex items-center gap-2.5 mb-1.5">
                <span className="badge num w-6 px-0 justify-center shrink-0">{i + 1}</span>
                <span className="text-xs text-heading truncate" title={d.name}>
                  {d.name}
                </span>
                <span className="ml-auto flex items-baseline gap-2 shrink-0">
                  <span className="num text-xs text-faint">{pct.toFixed(0)}%</span>
                  <span
                    className="num text-xs font-semibold text-heading"
                    title={zh ? `${d.invocation_count} 次调用` : `${d.invocation_count} invocations`}
                  >
                    {formatTokens(d.invocation_count)}
                  </span>
                </span>
              </div>
              <Meter ratio={maxCount > 0 ? d.invocation_count / maxCount : 0} chartIndex={chartIndex} />
            </div>
          );
        })}
      </div>

      {sorted.length > 10 && (
        <p className="text-xs text-faint mt-3">
          {zh ? `另有 ${sorted.length - 10} 个 Skill` : `+${sorted.length - 10} more skills`}
        </p>
      )}
    </Card>
  );
}
