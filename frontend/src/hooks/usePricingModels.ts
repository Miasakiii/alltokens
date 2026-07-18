import { useState, useEffect, useCallback, useMemo } from 'react';
import { api } from '../api/client';
import type { PricingEntry } from '../api/types';

/**
 * 加载全部定价条目（含 context_window），提供按 provider/model 查询上下文窗口大小的解析器。
 * 匹配策略镜像后端 PricingEngine::find：先精确匹配，再去掉版本号后缀做基础模型名回退。
 */
export function usePricingModels() {
  const [models, setModels] = useState<PricingEntry[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    api
      .pricingModels()
      .then((data) => {
        if (active) setModels(data);
      })
      .catch(() => {
        if (active) setModels([]);
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const byKey = useMemo(() => {
    const map = new Map<string, number>();
    for (const m of models) {
      if (m.context_window > 0) map.set(`${m.provider}/${m.model}`, m.context_window);
    }
    return map;
  }, [models]);

  const contextWindowFor = useCallback(
    (provider: string, model: string): number | undefined => {
      const exact = byKey.get(`${provider}/${model}`);
      if (exact) return exact;
      // 去掉最后一段 "-<suffix>"（如日期），回退到基础模型名
      const idx = model.lastIndexOf('-');
      if (idx > 0) {
        const base = model.slice(0, idx);
        const hit = byKey.get(`${provider}/${base}`);
        if (hit) return hit;
      }
      return undefined;
    },
    [byKey],
  );

  return { models, loading, contextWindowFor };
}
