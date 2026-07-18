import { useEffect, useState } from 'react';
import { formatAge } from '../../utils/dates';
import { useLang } from '../../i18n';

interface Props {
  lastUpdated: number | null;
  prefix?: string;
}

/**
 * 中文相对时间（"刚刚"、"5 秒前"、"3 分钟前"、"2 小时前"、"4 天前"）。
 * 与 utils/dates 的英文 formatAge 平行，供 zh 模式使用（utils/ 不可改动）。
 */
export function formatAgeZh(epochMs: number): string {
  const diff = Date.now() - epochMs;
  if (!Number.isFinite(diff) || diff < 0) return '刚刚';
  const secs = Math.floor(diff / 1000);
  if (secs < 5) return '刚刚';
  if (secs < 60) return `${secs} 秒前`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins} 分钟前`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  return `${days} 天前`;
}

/**
 * Self-ticking data-freshness label ("Updated 5m ago" / "更新于 5 分钟前").
 * Re-renders every 15s on its own so callers don't need a timer. Renders
 * nothing until the first successful load. Styled as faint small text.
 * `prefix` 未传时按当前语言取默认前缀（更新于 / Updated）。
 */
export default function FreshnessLabel({ lastUpdated, prefix }: Props) {
  const zh = useLang().lang === 'zh';
  const [, setTick] = useState(0);
  useEffect(() => {
    const timer = window.setInterval(() => setTick((n) => n + 1), 15000);
    return () => window.clearInterval(timer);
  }, []);

  if (lastUpdated === null) return null;
  const label = prefix ?? (zh ? '更新于' : 'Updated');
  return (
    <span className="text-faint num whitespace-nowrap">
      {label} {zh ? formatAgeZh(lastUpdated) : formatAge(lastUpdated)}
    </span>
  );
}
