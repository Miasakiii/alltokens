import type { CodexQuotaResponse } from '../api/types';
import { useCallback, useEffect, useState } from 'react';
import { api } from '../api/client';

export function useCodexQuota(refreshOnMount = false) {
  const [data, setData] = useState<CodexQuotaResponse | null>(null);
  const [loading, setLoading] = useState(true);

  const refetch = useCallback(async (refresh = false) => {
    setLoading(true);
    try {
      const response = await api.codexQuota(refresh);
      setData(response);
    } catch {
      setData(null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refetch(refreshOnMount);
  }, [refetch, refreshOnMount]);

  return { data, loading, refetch };
}
