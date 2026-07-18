// Shared numeric formatters — the single source of truth for token/cost/percent
// formatting. Components import from here rather than re-declaring local `fmt` copies.

/**
 * Format a token count with K/M/B tiers.
 *
 * - `< 1_000` -> plain integer (e.g. `842`)
 * - `< 1_000_000` -> K (e.g. `1.2K`)
 * - `< 1_000_000_000` -> M (e.g. `3.5M`)
 * - otherwise -> B (e.g. `2.1B`) — real-env probes show 2B+ tokens/day is realistic
 */
export function formatTokens(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return '0';
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return Math.round(n).toString();
}

/** Format a USD cost. Returns `—` for non-positive values, otherwise `$X.XXXX`. */
export function formatCost(usd: number): string {
  if (!Number.isFinite(usd) || usd <= 0) return '—';
  if (usd >= 1) return `$${usd.toFixed(2)}`;
  return `$${usd.toFixed(4)}`;
}

/** Format a ratio (0..1) as a percentage with `digits` fractional digits. */
export function formatPercent(ratio: number, digits = 1): string {
  if (!Number.isFinite(ratio)) return '—';
  return `${(ratio * 100).toFixed(digits)}%`;
}

/** Format an integer with locale grouping (e.g. `1,234,567`). Exact — no K/M/B rounding. */
export function formatInt(n: number): string {
  if (!Number.isFinite(n)) return '0';
  return Math.round(n).toLocaleString();
}
