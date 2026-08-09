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
      return response;
    } catch {
      setData(null);
      return null;
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    (async () => {
      // 先读缓存（快）；仅在无快照时才触发真实 RPC（spawn codex 子进程 ~6s），
      // 避免每次挂载都等待 6 秒
      const cached = await refetch(false);
      if (refreshOnMount && !cached?.snapshot) {
        await refetch(true);
      }
    })();
  }, [refetch, refreshOnMount]);

  return { data, loading, refetch };
}
