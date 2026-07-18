import { Meter } from './ui/primitives';
import type { BudgetConfig } from '../api/types';
import { useLang } from '../i18n';

interface Props {
  config: BudgetConfig | null;
  monthlyCostUsd: number;
  loading: boolean;
}

export default function BudgetAlert({ config, monthlyCostUsd, loading }: Props) {
  const zh = useLang().lang === 'zh';

  if (loading || !config?.enabled || !config.monthly_usd || config.monthly_usd <= 0) {
    return null;
  }

  const budget = config.monthly_usd;
  const ratio = monthlyCostUsd / budget;
  const pct = Math.min(ratio * 100, 999);

  if (ratio < 0.8) return null;

  const exceeded = ratio >= 1;
  const heading = exceeded
    ? zh
      ? '本月预算已超支'
      : 'Monthly budget exceeded'
    : zh
      ? '接近本月预算上限'
      : 'Approaching monthly budget';

  return (
    <div
      role="alert"
      className="flex items-center gap-3 rounded-xl border px-4 py-3 sm:gap-4"
      style={{
        background: exceeded ? 'var(--app-danger-soft)' : 'var(--app-warn-soft)',
        borderColor: exceeded ? 'var(--app-danger-soft)' : 'var(--app-warn-soft)',
      }}
    >
      <svg
        className={`w-5 h-5 shrink-0 ${exceeded ? 'text-danger' : 'text-warn'}`}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
      >
        <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
        <line x1="12" y1="9" x2="12" y2="13" />
        <line x1="12" y1="17" x2="12.01" y2="17" />
      </svg>

      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <p className="text-sm font-medium text-heading">{heading}</p>
          <span className={`badge num ${exceeded ? 'badge-danger' : 'badge-warn'}`}>
            {pct.toFixed(0)}%
          </span>
        </div>
        <p className="num mt-0.5 text-xs text-muted">
          {zh
            ? `本月已用 $${monthlyCostUsd.toFixed(2)} / $${budget.toFixed(2)}`
            : `$${monthlyCostUsd.toFixed(2)} of $${budget.toFixed(2)} used this month`}
        </p>
      </div>

      <div className="w-24 shrink-0 sm:w-28">
        <Meter ratio={ratio} tone={exceeded ? 'danger' : 'warn'} />
      </div>
    </div>
  );
}
