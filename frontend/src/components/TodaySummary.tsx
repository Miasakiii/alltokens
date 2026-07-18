import type { OverviewStats } from '../api/types';
import { formatTokens, formatCost } from '../utils/format';
import { useLang } from '../i18n';
import { Card, Meter, Skeleton } from './ui/primitives';

interface WindowData {
  stats: OverviewStats | null;
  loading: boolean;
}

interface Props {
  today: WindowData;
  week: WindowData;
  month: WindowData;
  subscription?: { feeTotal: number; enabled: boolean };
}

interface TileProps {
  label: string;
  stats: OverviewStats | null;
  loading: boolean;
  /** 分类标记色（语义 CSS 变量） */
  dot: string;
}

function Tile({ label, stats, loading, dot }: TileProps) {
  const zh = useLang().lang === 'zh';

  if (loading) {
    return (
      <div className="surface-2 px-4 py-3" aria-label={zh ? '加载中' : 'Loading'}>
        <Skeleton className="h-3 w-16" />
        <Skeleton className="mt-2.5 h-6 w-24" />
        <Skeleton className="mt-2 h-3 w-32" />
      </div>
    );
  }

  const cost = stats?.total_cost_usd ?? 0;
  const tokens = stats?.total_tokens ?? 0;
  const requests = stats?.total_requests ?? 0;

  return (
    <div className="surface-2 px-4 py-3 min-w-0">
      <div className="flex items-center gap-1.5">
        <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: dot }} />
        <p className="label-xs truncate">{label}</p>
      </div>
      <p className="num mt-1.5 text-xl font-semibold text-accent leading-tight truncate">
        {formatCost(cost)}
      </p>
      <p className="num mt-1 text-[11px] text-muted truncate">
        {formatTokens(tokens)} tokens · {formatTokens(requests)} {zh ? '次请求' : 'requests'}
        {stats != null && (
          <span className="text-success">
            {' · '}
            {(stats.cache_hit_rate * 100).toFixed(0)}% {zh ? '缓存命中' : 'cache hit'}
          </span>
        )}
      </p>
    </div>
  );
}

/**
 * "羊毛看板" — surfaces API-equivalent cost across today / this week / this month.
 * Because a subscription is a flat monthly fee, this accumulated pay-as-you-go
 * cost is effectively the value the subscription has already covered.
 */
export default function TodaySummary({ today, week, month, subscription }: Props) {
  const zh = useLang().lang === 'zh';
  const feeTotal = subscription?.feeTotal ?? 0;
  const showRecoup = (subscription?.enabled ?? false) && feeTotal > 0;
  const monthCost = month.stats?.total_cost_usd ?? 0;
  const recoupPercent = feeTotal > 0 ? (monthCost / feeTotal) * 100 : 0;
  const recouped = recoupPercent >= 100;

  return (
    <Card
      title={zh ? '订阅羊毛价值' : 'Subscription Value'}
      subtitle={
        zh
          ? '按当前价格表折算的 API 计价累计成本 · 订阅制下即为已省下的金额'
          : 'API-equivalent cost accrued at current pricing · the amount your subscription has already covered'
      }
    >
      <div className="space-y-3">
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
          <Tile
            label={zh ? '今日' : 'Today'}
            stats={today.stats}
            loading={today.loading}
            dot="var(--app-accent)"
          />
          <Tile
            label={zh ? '本周' : 'This Week'}
            stats={week.stats}
            loading={week.loading}
            dot="var(--app-info)"
          />
          <Tile
            label={zh ? '本月' : 'This Month'}
            stats={month.stats}
            loading={month.loading}
            dot="var(--app-success)"
          />
        </div>

        {showRecoup && (
          <div className="surface-2 px-4 py-3 space-y-2">
            <div className="flex items-center justify-between gap-2 flex-wrap">
              <p className="text-xs font-medium text-heading">
                {zh ? '本月回本' : 'Recouped this month'}{' '}
                <span className={`num ${recouped ? 'text-success' : 'text-accent'}`}>
                  {Math.round(recoupPercent)}%
                </span>
              </p>
              <p className="num text-[11px] text-muted">
                {formatCost(monthCost)} / {formatCost(feeTotal)}
              </p>
            </div>
            <Meter ratio={monthCost / feeTotal} tone={recouped ? 'success' : 'accent'} />
            <p className="text-[11px] text-muted">
              {recouped
                ? zh
                  ? `已回本 · 多省 ${formatCost(monthCost - feeTotal)}`
                  : `Fully recouped · ${formatCost(monthCost - feeTotal)} extra saved`
                : zh
                  ? `距回本还差 ${formatCost(feeTotal - monthCost)}`
                  : `${formatCost(feeTotal - monthCost)} away from break-even`}
            </p>
          </div>
        )}
      </div>
    </Card>
  );
}
