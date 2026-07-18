import type { ClaudeQuotaResponse } from '../api/types';
import { useCallback, useEffect, useState } from 'react';
import { api } from '../api/client';

export function useClaudeQuota(refreshOnMount = false) {
  const [data, setData] = useState<ClaudeQuotaResponse | null>(null);
  const [loading, setLoading] = useState(true);

  const refetch = useCallback(async (refresh = false) => {
    setLoading(true);
    try {
      const response = await api.claudeQuota(refresh);
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
