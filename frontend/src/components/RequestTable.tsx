import { useState } from 'react';
import type { UsageRecord } from '../api/types';
import { formatTokens } from '../utils/format';
import { useLang } from '../i18n';
import { Card, LoadingRows, EmptyState } from './ui/primitives';
import ReasoningBadge from './ReasoningBadge';
import RequestDetailModal from './RequestDetailModal';

interface Props {
  data: UsageRecord[] | null;
  loading: boolean;
  page?: number;
  pageSize?: number;
  total?: number;
  onPageChange?: (page: number) => void;
  contextWindowFor?: (provider: string, model: string) => number | undefined;
}

function timeAgo(ts: string, zh: boolean): string {
  const diff = Date.now() - new Date(ts).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return zh ? '刚刚' : 'just now';
  if (mins < 60) return zh ? `${mins} 分钟前` : `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return zh ? `${hours} 小时前` : `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return zh ? `${days} 天前` : `${days}d ago`;
}

export default function RequestTable({
  data,
  loading,
  page = 0,
  pageSize = 20,
  total = 0,
  onPageChange,
  contextWindowFor,
}: Props) {
  const zh = useLang().lang === 'zh';
  const [selected, setSelected] = useState<UsageRecord | null>(null);
  const totalPages = Math.max(1, Math.ceil(total / pageSize));
  const showPagination = onPageChange && total > pageSize;
  const rangeStart = total === 0 ? 0 : page * pageSize + 1;
  const rangeEnd = Math.min((page + 1) * pageSize, total);

  const cardTitle = zh ? '最近请求' : 'Recent Requests';

  const headerActions =
    !loading && data && data.length > 0 && total > 0 ? (
      <span className="text-xs text-faint num">
        {zh
          ? `第 ${rangeStart}–${rangeEnd} 条，共 ${total} 条`
          : `${rangeStart}–${rangeEnd} of ${total}`}
      </span>
    ) : null;

  if (loading) {
    return (
      <Card title={cardTitle}>
        <LoadingRows rows={5} />
      </Card>
    );
  }

  if (!data || data.length === 0) {
    return (
      <Card title={cardTitle}>
        <EmptyState
          title={zh ? '没有符合当前筛选条件的请求' : 'No requests match the current filters'}
          hint={zh ? '运行 `alltokens scan` 或调整筛选条件。' : 'Run `alltokens scan` or adjust filters.'}
        />
      </Card>
    );
  }

  return (
    <>
    <Card title={cardTitle} actions={headerActions} bodyClassName="px-4 pb-4 overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-[var(--app-surface-border)]">
            <th className="label-xs text-left py-2 pr-4">{zh ? '时间' : 'Time'}</th>
            <th className="label-xs text-left py-2 pr-4">Provider</th>
            <th className="label-xs text-left py-2 pr-4">Model</th>
            <th className="label-xs text-left py-2 pr-4 hidden sm:table-cell">Tool</th>
            <th className="label-xs text-right py-2 pr-4 hidden md:table-cell">{zh ? '输入' : 'Input'}</th>
            <th className="label-xs text-right py-2 pr-4 hidden md:table-cell">{zh ? '输出' : 'Output'}</th>
            <th className="label-xs text-right py-2 pr-4 hidden lg:table-cell">{zh ? '缓存' : 'Cache'}</th>
            <th className="label-xs text-right py-2">{zh ? '成本' : 'Cost'}</th>
          </tr>
        </thead>
        <tbody>
          {data.map((r, i) => (
            <tr
              key={r.id ?? i}
              className="border-b border-[var(--app-surface-border)] last:border-b-0 hover:bg-[var(--app-surface-2)] transition-colors cursor-pointer"
              onClick={() => setSelected(r)}
            >
              <td className="py-2.5 pr-4 text-muted text-xs whitespace-nowrap">{timeAgo(r.timestamp, zh)}</td>
              <td className="py-2.5 pr-4">
                <span className="badge">{r.provider}</span>
              </td>
              <td className="py-2.5 pr-4 text-heading text-xs max-w-[180px] truncate" title={r.model}>
                {r.model}
              </td>
              <td className="py-2.5 pr-4 text-muted text-xs hidden sm:table-cell">{r.tool || '-'}</td>
              <td className="py-2.5 pr-4 text-right text-xs num hidden md:table-cell">{formatTokens(r.input_tokens)}</td>
              <td className="py-2.5 pr-4 text-right text-xs num hidden md:table-cell">
                <div className="flex flex-col items-end">
                  <span>{formatTokens(r.output_tokens)}</span>
                  <ReasoningBadge n={r.reasoning_tokens} compact />
                </div>
              </td>
              <td className="py-2.5 pr-4 text-right text-info text-xs num hidden lg:table-cell">{formatTokens(r.cache_read_tokens)}</td>
              <td className="py-2.5 text-right text-success text-xs num">
                {r.cost_usd > 0 ? `$${r.cost_usd.toFixed(4)}` : '-'}
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      {showPagination && (
        <div className="flex items-center justify-end gap-2 mt-4">
          <button
            type="button"
            disabled={page === 0}
            onClick={() => onPageChange(page - 1)}
            className="btn"
          >
            {zh ? '上一页' : 'Previous'}
          </button>
          <span className="text-xs text-muted num">
            {zh ? `第 ${page + 1} / ${totalPages} 页` : `Page ${page + 1} of ${totalPages}`}
          </span>
          <button
            type="button"
            disabled={page + 1 >= totalPages}
            onClick={() => onPageChange(page + 1)}
            className="btn"
          >
            {zh ? '下一页' : 'Next'}
          </button>
        </div>
      )}
    </Card>
    <RequestDetailModal
      record={selected}
      onClose={() => setSelected(null)}
      contextWindow={
        selected && contextWindowFor ? contextWindowFor(selected.provider, selected.model) : undefined
      }
    />
    </>
  );
}
