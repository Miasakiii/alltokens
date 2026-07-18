// Shared segmented control — the single source for tab-style toggle groups
// (metric pickers, daily/weekly granularity, header period switcher).
// Styled with the design-system tokens in index.css (.pill / .pill-item).

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
}

/**
 * 语义化着色：active 项使用对应的柔和底色。
 * 'indigo' → 主强调色（copper），'emerald' → success，'violet' → info。
 */
export type SegmentedAccent = 'indigo' | 'emerald' | 'violet';

const ACTIVE_CLASS: Record<SegmentedAccent, string> = {
  indigo: 'bg-[var(--app-accent-soft)] text-accent font-semibold',
  emerald: 'bg-[var(--app-success-soft)] text-success font-semibold',
  violet: 'bg-[var(--app-info-soft)] text-info font-semibold',
};

interface Props<T extends string> {
  options: SegmentedOption<T>[];
  value: T;
  onChange: (value: T) => void;
  accent?: SegmentedAccent;
}

export default function SegmentedControl<T extends string>({
  options,
  value,
  onChange,
  accent = 'indigo',
}: Props<T>) {
  return (
    <div className="pill">
      {options.map((opt) => (
        <button
          key={opt.value}
          type="button"
          onClick={() => onChange(opt.value)}
          className={`pill-item ${value === opt.value ? ACTIVE_CLASS[accent] : ''}`}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}
