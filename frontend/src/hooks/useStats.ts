import { useState, useEffect, useCallback } from 'react';
import { api, type StatsQuery, type ListQuery } from '../api/client';

function useQuery<T>(fetcher: () => Promise<T>, deps: unknown[]) {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refetch = useCallback(() => {
    setLoading(true);
    setError(null);
    fetcher()
      .then(setData)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  }, deps);

  useEffect(refetch, [refetch]);

  return { data, loading, error, refetch };
}

function filterDeps(q?: StatsQuery | ListQuery) {
  return [q?.last, q?.start_date, q?.end_date, q?.provider, q?.model, q?.collector, q?.tool];
}

export function useOverview(q?: StatsQuery) {
  return useQuery(() => api.overview(q), filterDeps(q));
}

export function useProviders(q?: StatsQuery) {
  return useQuery(() => api.providers(q), [q?.last]);
}

export function useModels(q?: StatsQuery) {
  return useQuery(() => api.models(q), [q?.last]);
}

export function useTools(q?: StatsQuery) {
  return useQuery(() => api.tools(q), [q?.last]);
}

export function useProjects(q?: StatsQuery) {
  return useQuery(() => api.projects(q), [q?.last]);
}

export function useSessions(q?: StatsQuery) {
  return useQuery(() => api.sessions(q), filterDeps(q));
}

export function useToolsRanking(q?: StatsQuery) {
  return useQuery(() => api.toolsRanking(q), [q?.last]);
}

export function useSkillsRanking(q?: StatsQuery) {
  return useQuery(() => api.skillsRanking(q), [q?.last]);
}

export function useTrends(q?: StatsQuery) {
  return useQuery(() => api.trends(q), filterDeps(q));
}

export function useHeatmap(q?: StatsQuery) {
  return useQuery(() => api.heatmap({ days: 180, ...q }), [...filterDeps(q), q?.days]);
}

export function useHourOfWeek(q?: StatsQuery) {
  return useQuery(() => api.hourOfWeek(q), filterDeps(q));
}

export function useRequests(q?: ListQuery) {
  return useQuery(() => api.requests(q), [...filterDeps(q), q?.page, q?.page_size]);
}
