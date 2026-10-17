import { useState, useEffect, useCallback } from 'react';
import { api } from '../api/client';
import type { BudgetConfig } from '../api/types';

export function useBudgetConfig() {
  const [data, setData] = useState<BudgetConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refetch = useCallback(() => {
    setLoading(true);
    setError(null);
    api
      .budgetConfig()
      .then(setData)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    refetch();
  }, [refetch]);

  const save = useCallback(async (config: BudgetConfig) => {
    const saved = await api.setBudgetConfig(config);
    setData(saved);
    return saved;
  }, []);

  return { data, loading, error, refetch, save };
}
