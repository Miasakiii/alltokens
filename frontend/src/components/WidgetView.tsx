import { useMemo, useState } from 'react';
import { isTauriApp } from '../api/client';
import type { CodexQuotaWindow } from '../api/types';
import { useClaudeQuota } from '../hooks/useClaudeQuota';
import { useCodexQuota } from '../hooks/useCodexQuota';
import { useOverview, useTrends } from '../hooks/useStats';
import { useScanComplete } from '../hooks/useWebSocket';
import { useLang } from '../i18n';
import { formatAge, todayStartISO } from '../utils/dates';
import { formatTokens } from '../utils/format';
import { formatAgeZh } from './ui/FreshnessLabel';

declare global {
  interface Window {
    __TAURI__?: {
      core?: { invoke?: (cmd: string, args?: Record<string, unknown>) => Promise<unknown> };
    };
  }
}

function invoke(cmd: string, args?: Record<string, unknown>): void {
  void window.__TAURI__?.core?.invoke?.(cmd, args);
}

function remainingPercent(w: CodexQuotaWindow | null | undefined): number | null {
  if (!w) return null;
  return w.remaining_percent ?? (w.used_percent != null ? 100 - w.used_percent : null);
}

function quotaColor(remaining: number | null): string {
  if (remaining == null) return 'var(--app-faint)';
  if (remaining <= 20) return 'var(--app-danger)';
  if (remaining <= 50) return 'var(--app-warn)';
  return 'var(--app-success)';
}

function QuotaBar({ label, window: quotaWindow }: { label: string; window?: CodexQuotaWindow | null }) {
  const remaining = remainingPercent(quotaWindow);
  return (
    <div>
      <div className="flex items-center justify-between text-xs mb-1">
        <span className="text-muted">{label}</span>
        <span className="num text-heading font-medium">
          {remaining == null ? '--' : `${remaining}%`}
        </span>
      </div>
      <div className="meter !h-1.5">
        <div
          className="meter-fill transition-all"
          style={{ width: `${remaining ?? 0}%`, background: quotaColor(remaining) }}
        />
      </div>
    </div>
  );
}

/**
 * 桌面悬浮小组件视图（Tauri `widget` 窗口加载 `index.html?widget=1`）。
 * 今日成本 + Codex/Claude 额度条 + 近 7 天迷你趋势；数据走同源 API
 * （Tauri 下自动指向 127.0.0.1:3212），扫描完成经 WebSocket 实时刷新。
 */
export default function WidgetView() {
  const zh = useLang().lang === 'zh';
  const todayQuery = useMemo(() => ({ start_date: todayStartISO() }), []);
  const { data: today, refetch: refetchToday } = useOverview(todayQuery);
  const { data: trends, refetch: refetchTrends } = useTrends({ last: '7d' });
  const codex = useCodexQuota();
  const claude = useClaudeQuota();
  const [updatedAt, setUpdatedAt] = useState(() => Date.now());

  useScanComplete(() => {
    refetchToday();
    refetchTrends();
    codex.refetch();
    claude.refetch();
    setUpdatedAt(Date.now());
  });

  // 近 7 天柱图：按日期聚合 total_tokens，缺数据日补零
  const week = useMemo(() => {
    const sums = new Map<string, number>();
    for (const row of trends ?? []) {
      sums.set(row.date, (sums.get(row.date) ?? 0) + row.total_tokens);
    }
    const now = new Date();
    const days: { key: string; label: string; tokens: number }[] = [];
    for (let i = 6; i >= 0; i--) {
      const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - i);
      const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(
        d.getDate(),
      ).padStart(2, '0')}`;
      days.push({
        key,
        label: `${d.getMonth() + 1}/${d.getDate()}`,
        tokens: sums.get(key) ?? 0,
      });
    }
    return days;
  }, [trends]);
  const maxTokens = Math.max(...week.map((d) => d.tokens), 1);

  return (
    <div className="h-screen overflow-hidden flex flex-col p-2 select-none">
      {/* 标题栏：左侧为拖动区 */}
      <div className="flex items-center justify-between pl-2 pr-1 py-1">
        <div
          data-tauri-drag-region
          className="flex-1 py-1 text-xs font-semibold text-heading cursor-move"
        >
          AllTokens
        </div>
        {isTauriApp() && (
          <button
            onClick={() => invoke('set_widget_visible', { visible: false })}
            className="text-muted hover:text-heading text-sm leading-none px-1.5 py-1 rounded transition-colors"
            title={zh ? '隐藏小组件（托盘菜单「桌面小组件」可重新打开）' : 'Hide widget (reopen from the tray menu)'}
          >
            ×
          </button>
        )}
      </div>

      {/* 今日汇总 */}
      <div className="surface p-3 mb-2">
        <div className="label-xs">{zh ? '今日成本（API 等效）' : "Today's cost (API equivalent)"}</div>
        <div className="num text-2xl font-semibold tracking-tight text-heading mt-1">
          ${(today?.total_cost_usd ?? 0).toFixed(2)}
        </div>
        <div className="num text-xs text-muted mt-1">
          {formatTokens(today?.total_tokens ?? 0)} tokens · {today?.total_requests ?? 0}{' '}
          {zh ? '次请求' : 'requests'}
        </div>
      </div>

      {/* 额度剩余 */}
      <div className="surface p-3 mb-2 space-y-2.5">
        <div className="label-xs">{zh ? '额度剩余' : 'Quota remaining'}</div>
        <QuotaBar label="Codex 5h" window={codex.data?.snapshot?.five_hour} />
        <QuotaBar label="Codex 7d" window={codex.data?.snapshot?.seven_day} />
        <QuotaBar label="Claude 5h" window={claude.data?.snapshot?.five_hour} />
        <QuotaBar label="Claude 7d" window={claude.data?.snapshot?.seven_day} />
      </div>

      {/* 近 7 天迷你趋势 */}
      <div className="surface p-3 flex-1 flex flex-col min-h-0">
        <div className="label-xs mb-2">{zh ? '近 7 天' : 'Last 7 days'}</div>
        <div className="flex items-end gap-1 flex-1 min-h-[48px]">
          {week.map((d) => {
            const height = Math.max(4, (d.tokens / maxTokens) * 100);
            return (
              <div
                key={d.key}
                className="flex-1 flex flex-col items-center gap-1 group relative min-w-0"
              >
                <div className="absolute -top-9 left-1/2 -translate-x-1/2 theme-tooltip text-xs px-2 py-1 rounded-md opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-10 num">
                  {d.label} · {formatTokens(d.tokens)}
                </div>
                <div
                  className="w-full rounded-t-md opacity-55 group-hover:opacity-90 transition-opacity min-h-[4px]"
                  style={{ height: `${height}%`, background: 'var(--app-accent)' }}
                />
              </div>
            );
          })}
        </div>
        <div className="flex justify-between mt-1.5 text-[10px] text-faint num">
          <span>{week[0]?.label}</span>
          <span>{week[week.length - 1]?.label}</span>
        </div>
      </div>

      {/* 底部：新鲜度 + 打开 Dashboard */}
      <div className="flex items-center justify-between px-1 pt-2">
        <span className="text-[10px] text-faint">{zh ? formatAgeZh(updatedAt) : formatAge(updatedAt)}</span>
        {isTauriApp() && (
          <button
            onClick={() => invoke('open_main_window')}
            className="text-xs text-accent hover:opacity-75 transition-opacity"
          >
            {zh ? '打开 Dashboard →' : 'Open Dashboard →'}
          </button>
        )}
      </div>
    </div>
  );
}
