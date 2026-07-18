import type { OverviewStats } from '../api/types';
import { formatCost, formatTokens } from '../utils/format';
import { useLang } from '../i18n';
import FreshnessLabel from './ui/FreshnessLabel';

interface Props {
  stats: OverviewStats | null;
  connected: boolean;
  lastUpdated: number | null;
}

/**
 * Fixed bottom status bar: period totals on the left, WebSocket liveness +
 * data freshness on the right. Styled with the `.statusbar` design token.
 */
export default function StatusBar({ stats, connected, lastUpdated }: Props) {
  const zh = useLang().lang === 'zh';
  return (
    <footer className="fixed bottom-0 inset-x-0 z-30 statusbar">
      <div className="max-w-7xl mx-auto px-3 sm:px-6 h-9 flex items-center justify-between gap-4 text-xs text-muted">
        <div className="min-w-0 truncate num">
          {stats ? (
            <>
              <span>
                {zh
                  ? `${formatTokens(stats.total_requests)} 次请求`
                  : `${formatTokens(stats.total_requests)} requests`}
              </span>
              <span className="mx-1.5 text-faint">·</span>
              <span>{formatTokens(stats.total_tokens)} tokens</span>
              <span className="mx-1.5 text-faint">·</span>
              <span>{formatCost(stats.total_cost_usd)}</span>
            </>
          ) : (
            <span>—</span>
          )}
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <span
            className="w-1.5 h-1.5 rounded-full shrink-0"
            style={{
              background: connected ? 'var(--app-success)' : 'var(--app-danger)',
              boxShadow: connected ? '0 0 0 3px var(--app-success-soft)' : 'none',
            }}
            aria-hidden="true"
          />
          <span className={connected ? 'text-muted' : 'text-danger'}>
            {connected ? (zh ? '已连接' : 'Live') : zh ? '离线' : 'Offline'}
          </span>
          <span className="hidden sm:inline">
            <FreshnessLabel lastUpdated={lastUpdated} prefix={zh ? '· 更新于' : '· Updated'} />
          </span>
        </div>
      </div>
    </footer>
  );
}
