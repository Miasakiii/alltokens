import type { SessionStats } from '../api/types';
import { formatTokens, formatCost } from '../utils/format';
import { useLang } from '../i18n';
import { Card, LoadingRows, EmptyState } from './ui/primitives';

interface Props {
  data: SessionStats[] | null;
  loading: boolean;
}

/** Format a duration in seconds as a compact `Xh Ym` / `Ym Zs` / `Zs` string. */
function formatDuration(secs: number, zh: boolean): string {
  if (!Number.isFinite(secs) || secs <= 0) return '—';
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  if (h > 0) return zh ? `${h} 小时 ${m} 分` : `${h}h ${m}m`;
  if (m > 0) return zh ? `${m} 分 ${s} 秒` : `${m}m ${s}s`;
  return zh ? `${s} 秒` : `${s}s`;
}

/** Relative "time ago" from an ISO timestamp. */
function timeAgo(ts: string, zh: boolean): string {
  const t = new Date(ts).getTime();
  if (!Number.isFinite(t)) return '—';
  const diff = Date.now() - t;
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return zh ? '刚刚' : 'just now';
  if (mins < 60) return zh ? `${mins} 分钟前` : `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return zh ? `${hours} 小时前` : `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return zh ? `${days} 天前` : `${days}d ago`;
}

/** Trim long session ids (paths / uuids) to a readable tail. */
function shortSession(id: string): string {
  const tail = id.split(/[\\/]/).pop() ?? id;
  return tail.length > 28 ? `…${tail.slice(-27)}` : tail;
}

export default function SessionBreakdown({ data, loading }: Props) {
  const zh = useLang().lang === 'zh';

  if (loading || !data || data.length === 0) {
    return (
      <Card title={zh ? '会话明细' : 'Session Details'}>
        {loading ? (
          <LoadingRows rows={6} />
        ) : (
          <EmptyState
            title={zh ? '暂无带 session_id 的会话数据' : 'No session data with session_id yet'}
          />
        )}
      </Card>
    );
  }

  const sorted = [...data].sort(
    (a, b) => new Date(b.last_seen).getTime() - new Date(a.last_seen).getTime(),
  );

  return (
    <Card
      title={zh ? '会话明细' : 'Session Details'}
      actions={
        <span className="badge num">
          {zh ? `${sorted.length} 个会话` : `${sorted.length} sessions`}
        </span>
      }
    >
      <div className="overflow-x-auto -mx-1">
        <table className="w-full text-xs">
          <thead>
            <tr className="border-b border-[var(--app-surface-border)]">
              <th className="label-xs text-left py-2 px-1">{zh ? '会话' : 'Session'}</th>
              <th className="label-xs text-left py-2 px-1">{zh ? '模型' : 'Model'}</th>
              <th className="label-xs text-right py-2 px-1">{zh ? '请求数' : 'Reqs'}</th>
              <th className="label-xs text-right py-2 px-1">Tokens</th>
              <th className="label-xs text-right py-2 px-1">{zh ? '成本' : 'Cost'}</th>
              <th className="label-xs text-right py-2 px-1">{zh ? '时长' : 'Duration'}</th>
              <th className="label-xs text-right py-2 px-1">{zh ? '最近活跃' : 'Last active'}</th>
            </tr>
          </thead>
          <tbody>
            {sorted.map((s) => (
              <tr
                key={`${s.provider}/${s.model}/${s.session_id}`}
                className="border-b border-[var(--app-surface-border)] last:border-0 hover:bg-[var(--app-surface-2)] transition-colors"
              >
                <td
                  className="py-2 px-1 text-heading font-mono truncate max-w-[10rem]"
                  title={s.session_id}
                >
                  {shortSession(s.session_id)}
                </td>
                <td className="py-2 px-1">
                  <span className="text-faint">{s.provider}</span>
                  <span className="text-muted"> · {s.model}</span>
                </td>
                <td className="py-2 px-1 text-right num text-muted">{s.request_count}</td>
                <td className="py-2 px-1 text-right num text-heading">{formatTokens(s.total_tokens)}</td>
                <td className="py-2 px-1 text-right num text-success">{formatCost(s.total_cost_usd)}</td>
                <td className="py-2 px-1 text-right num text-muted">{formatDuration(s.duration_secs, zh)}</td>
                <td className="py-2 px-1 text-right text-faint whitespace-nowrap" title={s.last_seen}>
                  {timeAgo(s.last_seen, zh)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Card>
  );
}
