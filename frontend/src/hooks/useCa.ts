import { useState, useEffect, useCallback } from 'react';
import { api } from '../api/client';
import type { CaStatus } from '../api/types';

export function useCa() {
  const [data, setData] = useState<CaStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refetch = useCallback(() => {
    setLoading(true);
    setError(null);
    api.caStatus().then(setData).catch((e) => setError(e.message)).finally(() => setLoading(false));
  }, []);

  useEffect(() => { refetch(); }, [refetch]);

  const install = useCallback(async (confirm: boolean) => {
    setBusy(true);
    setError(null);
    try {
      setData(await api.installCa(confirm));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const uninstall = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setData(await api.uninstallCa());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  return { data, loading, error, busy, refetch, install, uninstall };
}
