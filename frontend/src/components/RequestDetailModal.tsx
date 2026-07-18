import { type ReactNode } from 'react';
import type { UsageRecord } from '../api/types';
import { formatInt } from '../utils/format';
import { useLang } from '../i18n';

interface Props {
  record: UsageRecord | null;
  onClose: () => void;
  contextWindow?: number;
}

function Field({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex flex-col gap-1 min-w-0">
      <dt className="label-xs">{label}</dt>
      <dd className="text-sm text-heading break-all num">{value ?? '—'}</dd>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div>
      <p className="label-xs mb-2 pb-1.5 border-b border-[var(--app-surface-border)]">{title}</p>
      {children}
    </div>
  );
}

export default function RequestDetailModal({ record, onClose, contextWindow }: Props) {
  const zh = useLang().lang === 'zh';
  if (!record) return null;

  const ctxPct =
    contextWindow && contextWindow > 0 && record.total_tokens > 0
      ? Math.min(100, (record.total_tokens / contextWindow) * 100)
      : null;
  const ctxColor =
    ctxPct == null
      ? ''
      : ctxPct >= 90
        ? 'var(--app-danger)'
        : ctxPct >= 70
          ? 'var(--app-warn)'
          : 'var(--app-success)';

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ background: 'rgba(0,0,0,0.45)' }}
      onClick={onClose}
      role="presentation"
    >
      <div
        className="surface w-full max-w-lg max-h-[90vh] sm:max-h-[85vh] overflow-hidden flex flex-col mx-2 sm:mx-0"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="request-detail-title"
      >
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-[var(--app-surface-border)]">
          <h2 id="request-detail-title" className="label-xs">
            {zh ? '请求详情' : 'Request Detail'}
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="icon-btn"
            aria-label={zh ? '关闭' : 'Close'}
          >
            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>

        <div className="overflow-y-auto px-5 py-4 space-y-5">
          <Section title={zh ? '概览' : 'Overview'}>
            <dl className="grid grid-cols-1 sm:grid-cols-2 gap-3">
              <Field
                label={zh ? '时间' : 'Time'}
                value={new Date(record.timestamp).toLocaleString(zh ? 'zh-CN' : 'en-US')}
              />
              <Field label="Provider" value={record.provider} />
              <Field label="Model" value={record.model} />
              <Field label="Tool" value={record.tool} />
              <Field label="Collector" value={record.collector} />
            </dl>
          </Section>

          <Section title="Tokens">
            <dl className="grid grid-cols-1 sm:grid-cols-2 gap-3">
              <Field label={zh ? '输入 tokens' : 'Input tokens'} value={formatInt(record.input_tokens)} />
              <Field label={zh ? '输出 tokens' : 'Output tokens'} value={formatInt(record.output_tokens)} />
              <Field
                label={zh ? '推理 tokens' : 'Reasoning tokens'}
                value={record.reasoning_tokens > 0 ? formatInt(record.reasoning_tokens) : '—'}
              />
              <Field label={zh ? '缓存读取' : 'Cache read'} value={formatInt(record.cache_read_tokens)} />
              <Field label={zh ? '缓存创建' : 'Cache creation'} value={formatInt(record.cache_creation_tokens)} />
              <Field label={zh ? '总 tokens' : 'Total tokens'} value={formatInt(record.total_tokens)} />
            </dl>
          </Section>

          <Section title={zh ? '成本与性能' : 'Cost & Performance'}>
            <dl className="grid grid-cols-1 sm:grid-cols-2 gap-3">
              <Field label={zh ? '成本 (USD)' : 'Cost (USD)'} value={record.cost_usd > 0 ? `$${record.cost_usd.toFixed(6)}` : '—'} />
              <Field label={zh ? '成本 (CNY)' : 'Cost (CNY)'} value={record.cost_cny > 0 ? `¥${record.cost_cny.toFixed(4)}` : '—'} />
              <Field label={zh ? '延迟' : 'Latency'} value={record.latency_ms != null ? `${record.latency_ms} ms` : '—'} />
              <Field label={zh ? '流式' : 'Stream'} value={record.is_stream ? (zh ? '是' : 'Yes') : zh ? '否' : 'No'} />
              <Field label={zh ? '状态' : 'Status'} value={record.status_code ?? '—'} />
            </dl>
          </Section>

          <Section title={zh ? '标识符' : 'Identifiers'}>
            <dl className="grid grid-cols-1 sm:grid-cols-2 gap-3">
              <Field label={zh ? '会话 ID' : 'Session ID'} value={record.session_id} />
              <Field label={zh ? '请求 ID' : 'Request ID'} value={record.request_id} />
              <Field label={zh ? '来源文件' : 'Source file'} value={record.source_file} />
            </dl>
          </Section>

          {ctxPct != null && contextWindow && (
            <div>
              <div className="flex items-center justify-between mb-1.5">
                <span className="label-xs">{zh ? '上下文窗口' : 'Context window'}</span>
                <span className="text-xs text-muted num">
                  {ctxPct.toFixed(1)}% · {formatInt(record.total_tokens)} / {formatInt(contextWindow)}
                </span>
              </div>
              <div className="meter">
                <div className="meter-fill" style={{ width: `${ctxPct}%`, background: ctxColor }} />
              </div>
            </div>
          )}

          {record.raw_json && (
            <div>
              <p className="label-xs mb-1.5">{zh ? '原始 JSON' : 'Raw JSON'}</p>
              <pre className="surface-2 text-xs p-3 overflow-x-auto text-muted font-mono whitespace-pre-wrap break-all">
                {record.raw_json}
              </pre>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
