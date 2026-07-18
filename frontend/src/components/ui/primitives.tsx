import type { ReactNode } from 'react';
import { useLang } from '../../i18n';

/**
 * AllTokens 共享 UI 原语 —— 所有卡片/面板统一使用这些组件，
 * 保证标题层级、留白、加载态、空态一致。样式 token 见 index.css。
 */

interface CardProps {
  /** 卡片标题（可选）。提供时用统一标题层级渲染 */
  title?: ReactNode;
  /** 标题下方的辅助说明 */
  subtitle?: ReactNode;
  /** 标题行右侧的操作区 */
  actions?: ReactNode;
  className?: string;
  /** 内容区额外 className（默认 p-4；title 存在时内容为 px-4 pb-4） */
  bodyClassName?: string;
  children: ReactNode;
}

export function Card({ title, subtitle, actions, className = '', bodyClassName, children }: CardProps) {
  const hasHeader = title != null || actions != null;
  return (
    <section className={`surface min-w-0 ${className}`}>
      {hasHeader && (
        <header className="flex items-start justify-between gap-3 px-4 pt-3.5 pb-3">
          <div className="min-w-0">
            {title != null && <h3 className="label-xs truncate">{title}</h3>}
            {subtitle != null && <p className="mt-1 text-xs text-muted truncate">{subtitle}</p>}
          </div>
          {actions != null && <div className="flex items-center gap-2 shrink-0">{actions}</div>}
        </header>
      )}
      <div className={bodyClassName ?? (hasHeader ? 'px-4 pb-4' : 'p-4')}>{children}</div>
    </section>
  );
}

export function Skeleton({ className = '' }: { className?: string }) {
  return <div className={`skeleton ${className}`} aria-hidden="true" />;
}

/** 统一的加载占位：几行骨架条 */
export function LoadingRows({ rows = 3, className = '' }: { rows?: number; className?: string }) {
  return (
    <div className={`space-y-2.5 ${className}`} aria-label="Loading">
      {Array.from({ length: rows }).map((_, i) => (
        <Skeleton key={i} className={`h-3.5 ${i % 2 === 0 ? 'w-full' : 'w-2/3'}`} />
      ))}
    </div>
  );
}

interface EmptyStateProps {
  title?: string;
  hint?: string;
  className?: string;
}export function EmptyState({ title, hint, className = '' }: EmptyStateProps) {
  const zh = useLang().lang === 'zh';
  const text = title ?? (zh ? '暂无数据' : 'No data yet');
  return (
    <div className={`flex flex-col items-center justify-center gap-1 py-8 text-center ${className}`}>
      <svg
        className="w-6 h-6 text-faint mb-1"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
      >
        <rect x="3" y="3" width="18" height="18" rx="3" />
        <path d="M8 12h8M8 16h5" strokeLinecap="round" />
      </svg>
      <p className="text-sm text-muted">{text}</p>
      {hint && <p className="text-xs text-faint">{hint}</p>}
    </div>
  );
}

interface StatProps {
  label: ReactNode;
  value: ReactNode;
  /** 数值下方的辅助信息（同比、占比等） */
  hint?: ReactNode;
  /** 数值着色 */
  tone?: 'default' | 'accent' | 'success' | 'warn' | 'danger' | 'info';
  className?: string;
}

const TONE_CLASS: Record<NonNullable<StatProps['tone']>, string> = {
  default: 'text-heading',
  accent: 'text-accent',
  success: 'text-success',
  warn: 'text-warn',
  danger: 'text-danger',
  info: 'text-info',
};

export function Stat({ label, value, hint, tone = 'default', className = '' }: StatProps) {
  return (
    <div className={`min-w-0 ${className}`}>
      <div className="label-xs truncate">{label}</div>
      <div className={`num mt-1.5 text-2xl font-semibold tracking-tight truncate ${TONE_CLASS[tone]}`}>
        {value}
      </div>
      {hint != null && <div className="mt-1 text-xs text-muted truncate">{hint}</div>}
    </div>
  );
}

interface MeterProps {
  /** 0 - 1 之间的占比 */
  ratio: number;
  tone?: 'accent' | 'success' | 'warn' | 'danger' | 'info';
  /** 使用图表分类色（1-8），设置后覆盖 tone */
  chartIndex?: 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8;
  className?: string;
}

const METER_TONE: Record<NonNullable<MeterProps['tone']>, string> = {
  accent: 'var(--app-accent)',
  success: 'var(--app-success)',
  warn: 'var(--app-warn)',
  danger: 'var(--app-danger)',
  info: 'var(--app-info)',
};

export function Meter({ ratio, tone = 'accent', chartIndex, className = '' }: MeterProps) {
  const pct = Math.max(0, Math.min(1, ratio)) * 100;
  const color = chartIndex ? `var(--chart-${chartIndex})` : METER_TONE[tone];
  return (
    <div className={`meter ${className}`} role="progressbar" aria-valuenow={Math.round(pct)}>
      <div className="meter-fill" style={{ width: `${pct}%`, background: color }} />
    </div>
  );
}
