import { useState, useEffect, useCallback } from 'react';
import { api } from '../api/client';
import type { SubscriptionConfig } from '../api/types';

export function useSubscriptionConfig() {
  const [data, setData] = useState<SubscriptionConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refetch = useCallback(() => {
    setLoading(true);
    setError(null);
    api
      .subscriptionConfig()
      .then(setData)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    refetch();
  }, [refetch]);

  const save = useCallback(async (config: SubscriptionConfig) => {
    const saved = await api.setSubscriptionConfig(config);
    setData(saved);
    return saved;
  }, []);

  return { data, loading, error, refetch, save };
}
