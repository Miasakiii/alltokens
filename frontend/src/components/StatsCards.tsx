import type { OverviewStats } from '../api/types';
import { formatTokens } from '../utils/format';
import { useLang } from '../i18n';
import { Card, Skeleton, Stat } from './ui/primitives';

interface Props {
  stats: OverviewStats | null;
  loading: boolean;
}

type IconType = 'token' | 'cost' | 'request' | 'cache';
type Tone = 'default' | 'accent' | 'success' | 'info';

function StatIcon({ type }: { type: IconType }) {
  const cls = 'w-3.5 h-3.5 shrink-0';
  switch (type) {
    case 'token':
      return (
        <svg className={cls} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
        </svg>
      );
    case 'cost':
      return (
        <svg className={cls} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <line x1="12" y1="1" x2="12" y2="23" />
          <path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" />
        </svg>
      );
    case 'request':
      return (
        <svg className={cls} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <polyline points="22 12 18 12 15 21 9 3 6 12 2 12" />
        </svg>
      );
    case 'cache':
      return (
        <svg className={cls} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <ellipse cx="12" cy="5" rx="9" ry="3" />
          <path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3" />
          <path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5" />
        </svg>
      );
    default:
      return null;
  }
}

/** 图标跟随数值的语义色调，保持原版的色彩辨识度 */
const ICON_TONE: Record<Tone, string> = {
  default: 'text-faint',
  accent: 'text-accent',
  success: 'text-success',
  info: 'text-info',
};

interface StatCardProps {
  icon: IconType;
  label: string;
  value: string;
  hint?: string;
  tone: Tone;
}

function StatCard({ icon, label, value, hint, tone }: StatCardProps) {
  return (
    <Card>
      <Stat
        label={
          <span className="inline-flex items-center gap-1.5">
            <span className={ICON_TONE[tone]}>
              <StatIcon type={icon} />
            </span>
            {label}
          </span>
        }
        value={value}
        hint={hint}
        tone={tone}
      />
    </Card>
  );
}

/** 加载态：与真实卡片同构的 Skeleton 版卡片 */
function SkeletonCard() {
  return (
    <Card>
      <Skeleton className="h-3 w-20" />
      <Skeleton className="mt-3 h-7 w-24" />
      <Skeleton className="mt-2.5 h-3 w-28" />
    </Card>
  );
}

export default function StatsCards({ stats, loading }: Props) {
  const zh = useLang().lang === 'zh';

  if (loading || !stats) {
    return (
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-3 sm:gap-4">
        {[0, 1, 2, 3].map((i) => (
          <SkeletonCard key={i} />
        ))}
      </div>
    );
  }

  const inOut = zh
    ? `${formatTokens(stats.total_input_tokens)} 输入 / ${formatTokens(stats.total_output_tokens)} 输出`
    : `${formatTokens(stats.total_input_tokens)} in / ${formatTokens(stats.total_output_tokens)} out`;
  const tokensSub =
    stats.total_reasoning_tokens > 0
      ? zh
        ? `${inOut} / ${formatTokens(stats.total_reasoning_tokens)} 推理`
        : `${inOut} / ${formatTokens(stats.total_reasoning_tokens)} reasoning`
      : inOut;

  return (
    <div className="grid grid-cols-2 lg:grid-cols-4 gap-3 sm:gap-4">
      <StatCard
        icon="token"
        label={zh ? '总 Tokens' : 'Total Tokens'}
        value={formatTokens(stats.total_tokens)}
        hint={tokensSub}
        tone="default"
      />
      <StatCard
        icon="cost"
        label={zh ? '成本 (USD)' : 'Cost (USD)'}
        value={`$${stats.total_cost_usd.toFixed(4)}`}
        hint={zh ? `¥${stats.total_cost_cny.toFixed(2)}` : `¥${stats.total_cost_cny.toFixed(2)} CNY`}
        tone="accent"
      />
      <StatCard
        icon="request"
        label={zh ? '请求数' : 'Requests'}
        value={formatTokens(stats.total_requests)}
        hint={
          zh
            ? `成功率 ${(stats.success_rate * 100).toFixed(1)}%`
            : `${(stats.success_rate * 100).toFixed(1)}% success`
        }
        tone="info"
      />
      <StatCard
        icon="cache"
        label={zh ? '缓存命中率' : 'Cache Hit Rate'}
        value={`${(stats.cache_hit_rate * 100).toFixed(1)}%`}
        hint={
          zh
            ? `缓存读取 ${formatTokens(stats.total_cache_read_tokens)}`
            : `${formatTokens(stats.total_cache_read_tokens)} cached`
        }
        tone="success"
      />
    </div>
  );
}
