// Shared date helpers for stats queries that need a rolling window's start_date.
// All helpers return ISO-8601 strings that the Rust web layer (StatsQuery) will
// accept as `?start_date=...`.

/** Start of the current local day, expressed as an ISO-8601 UTC timestamp. */
export function todayStartISO(): string {
  const now = new Date();
  return new Date(now.getFullYear(), now.getMonth(), now.getDate(), 0, 0, 0, 0).toISOString();
}

/**
 * Start of the current week (Monday, local time), expressed as ISO-8601 UTC.
 * Monday-start matches the way most subscription rolling windows report weekly
 * quota (Codex `seven_day`, Claude Pro reset).
 */
export function weekStartISO(): string {
  const now = new Date();
  const day = now.getDay(); // 0 = Sun, 1 = Mon, ..., 6 = Sat
  const daysFromMonday = (day + 6) % 7;
  const monday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - daysFromMonday, 0, 0, 0, 0);
  return monday.toISOString();
}

/** First day of the current local month, expressed as ISO-8601 UTC. */
export function monthStartISO(): string {
  const now = new Date();
  return new Date(now.getFullYear(), now.getMonth(), 1, 0, 0, 0, 0).toISOString();
}

/**
 * Compact relative age of a past epoch-ms timestamp ("just now", `5s ago`,
 * `3m ago`, `2h ago`, `4d ago`). Used for data-freshness indicators.
 */
export function formatAge(epochMs: number): string {
  const diff = Date.now() - epochMs;
  if (!Number.isFinite(diff) || diff < 0) return 'just now';
  const secs = Math.floor(diff / 1000);
  if (secs < 5) return 'just now';
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}
