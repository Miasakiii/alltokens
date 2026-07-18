import { formatTokens, formatInt } from '../utils/format';
import { useLang } from '../i18n';

interface Props {
  /** Reasoning token count. Non-positive values render nothing. */
  n: number;
  /** Optional variant for tighter placement (e.g. inside a table cell). */
  compact?: boolean;
}

/**
 * Compact badge surfacing reasoning tokens for models that emit them (o-series,
 * Codex reasoning models, etc.). Silently renders nothing when `n <= 0` so the
 * UI stays quiet for non-reasoning traffic.
 */
export default function ReasoningBadge({ n, compact = false }: Props) {
  const zh = useLang().lang === 'zh';
  if (!Number.isFinite(n) || n <= 0) return null;

  const label = zh ? `+${formatTokens(n)} 推理` : `+${formatTokens(n)} reasoning`;
  const cls = compact
    ? 'inline-block text-[10px] leading-none num text-info mt-0.5'
    : 'badge badge-info num';

  return (
    <span
      className={cls}
      title={zh ? `${formatInt(n)} 推理 tokens` : `${formatInt(n)} reasoning tokens`}
    >
      {compact ? label : zh ? `推理: ${formatTokens(n)}` : `R: ${formatTokens(n)}`}
    </span>
  );
}
