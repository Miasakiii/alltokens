import { Card, EmptyState, Meter, Skeleton } from './ui/primitives';
import type { ClaudeQuotaSnapshot, CodexQuotaWindow } from '../api/types';
import { useLang } from '../i18n';

interface Props {
  snapshot: ClaudeQuotaSnapshot | null | undefined;
  error?: string | null;
  loading: boolean;
}

/** 从窗口数据推导剩余百分比（与原 formatWindow 逻辑一致） */
function remainingOf(window: CodexQuotaWindow | null | undefined): number | null {
  if (!window) return null;
  if (window.remaining_percent != null) return window.remaining_percent;
  if (window.used_percent != null) return 100 - window.used_percent;
  return null;
}

type Tone = 'success' | 'warn' | 'danger';

const TONE_TEXT: Record<Tone, string> = {
  success: 'text-success',
  warn: 'text-warn',
  danger: 'text-danger',
};

/** 剩余充足 success / 紧张 warn / 即将耗尽 danger */
function toneOf(remaining: number): Tone {
  if (remaining >= 50) return 'success';
  if (remaining >= 20) return 'warn';
  return 'danger';
}

function formatReset(resetsAt: number | null | undefined, zh: boolean): string | null {
  if (!resetsAt) return null;
  // 兼容秒 / 毫秒两种 unix 时间戳
  const ms = resetsAt > 1e12 ? resetsAt : resetsAt * 1000;
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return null;
  const hhmm = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  const isToday = d.toDateString() === new Date().toDateString();
  if (zh) {
    if (isToday) return `重置于 ${hhmm}`;
    return `重置于 ${d.getMonth() + 1}月${d.getDate()}日 ${hhmm}`;
  }
  if (isToday) return `Resets at ${hhmm}`;
  return `Resets on ${d.getMonth() + 1}/${d.getDate()} ${hhmm}`;
}

function QuotaWindow({ label, window }: { label: string; window: CodexQuotaWindow | null | undefined }) {
  const zh = useLang().lang === 'zh';
  const remaining = remainingOf(window);
  const reset = formatReset(window?.resets_at, zh);
  const tone: Tone = remaining != null ? toneOf(remaining) : 'success';

  return (
    <div className="surface-2 min-w-0 p-3">
      <div className="flex items-center justify-between gap-2">
        <span className="label-xs">{label}</span>
        {reset && <span className="num truncate text-[11px] text-faint">{reset}</span>}
      </div>
      {remaining != null ? (
        <>
          <div className={`num mt-2 text-xl font-semibold tracking-tight ${TONE_TEXT[tone]}`}>
            {Math.round(remaining)}
            <span className="text-sm font-medium">%</span>
            <span className="ml-1.5 text-xs font-normal text-muted">
              {zh ? '剩余' : 'remaining'}
            </span>
          </div>
          <Meter ratio={remaining / 100} tone={tone} className="mt-2.5" />
        </>
      ) : (
        <div className="num mt-2 text-xl font-semibold text-faint">--</div>
      )}
    </div>
  );
}

export default function ClaudeQuotaCard({ snapshot, error, loading }: Props) {
  const zh = useLang().lang === 'zh';

  if (loading) {
    return (
      <Card title={zh ? '账户额度 · Claude Code' : 'Account Quota · Claude Code'}>
        <div className="grid grid-cols-2 gap-3">
          <Skeleton className="h-20" />
          <Skeleton className="h-20" />
        </div>
      </Card>
    );
  }

  if (!snapshot && !error) return null;

  return (
    <Card
      title={zh ? '账户额度 · Claude Code' : 'Account Quota · Claude Code'}
      actions={
        snapshot?.is_stale ? (
          <span className="badge badge-warn">{zh ? '数据过期' : 'stale'}</span>
        ) : undefined
      }
    >
      {snapshot ? (
        <div className="grid grid-cols-2 gap-3">
          <QuotaWindow label={zh ? '5 小时' : '5 Hours'} window={snapshot.five_hour} />
          <QuotaWindow label={zh ? '7 天' : '7 Days'} window={snapshot.seven_day} />
        </div>
      ) : (
        <EmptyState title={zh ? '额度不可用' : 'Quota unavailable'} hint={error ?? undefined} />
      )}
    </Card>
  );
}
