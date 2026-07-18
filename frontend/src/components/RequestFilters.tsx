import { useLang } from '../i18n';

export interface RequestFilterValues {
  provider: string;
  model: string;
  collector: string;
  tool: string;
}

interface Props {
  values: RequestFilterValues;
  providers: string[];
  models: string[];
  onChange: (values: RequestFilterValues) => void;
}

export default function RequestFilters({ values, providers, models, onChange }: Props) {
  const zh = useLang().lang === 'zh';
  const set = (key: keyof RequestFilterValues, value: string) =>
    onChange({ ...values, [key]: value });

  const hasFilters = Object.values(values).some((v) => v !== '');

  const activeChips = (Object.keys(values) as (keyof RequestFilterValues)[]).filter(
    (key) => values[key] !== '',
  );

  return (
    <div className="surface p-4">
      <div className="flex items-center justify-between mb-3">
        <h3 className="label-xs">{zh ? '筛选请求' : 'Filter Requests'}</h3>
        {hasFilters && (
          <button
            type="button"
            onClick={() => onChange({ provider: '', model: '', collector: '', tool: '' })}
            className="btn"
          >
            {zh ? '清除全部' : 'Clear all'}
          </button>
        )}
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
        <div>
          <div className="label-xs mb-1.5">Provider</div>
          <select
            value={values.provider}
            onChange={(e) => set('provider', e.target.value)}
            className="input w-full"
          >
            <option value="">{zh ? '全部 Provider' : 'All providers'}</option>
            {providers.map((p) => (
              <option key={p} value={p}>
                {p}
              </option>
            ))}
          </select>
        </div>

        <div>
          <div className="label-xs mb-1.5">Model</div>
          <input
            type="text"
            list="model-options"
            value={values.model}
            onChange={(e) => set('model', e.target.value)}
            placeholder={zh ? '任意 Model' : 'Any model'}
            className="input w-full"
          />
          <datalist id="model-options">
            {models.map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
        </div>

        <div>
          <div className="label-xs mb-1.5">Collector</div>
          <input
            type="text"
            value={values.collector}
            onChange={(e) => set('collector', e.target.value)}
            placeholder={zh ? '例如 claude_code' : 'e.g. claude_code'}
            className="input w-full"
          />
        </div>

        <div>
          <div className="label-xs mb-1.5">Tool</div>
          <input
            type="text"
            value={values.tool}
            onChange={(e) => set('tool', e.target.value)}
            placeholder={zh ? '例如 cursor' : 'e.g. cursor'}
            className="input w-full"
          />
        </div>
      </div>

      {activeChips.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 mt-3">
          {activeChips.map((key) => (
            <button
              key={key}
              type="button"
              onClick={() => set(key, '')}
              title={zh ? `清除 ${key} 筛选` : `Clear ${key} filter`}
              className="badge badge-accent cursor-pointer hover:brightness-105 transition"
            >
              <span className="opacity-70">{key}:</span>
              <span className="max-w-[12rem] truncate">{values[key]}</span>
              <svg className="w-3 h-3 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
