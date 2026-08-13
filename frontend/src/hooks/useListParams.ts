import * as React from 'react';
import type { ListParams } from '@/types';

/**
 * Page / sort / filter state for a list screen. Changing any filter resets the
 * page — otherwise a narrowed result set can leave you stranded on page 7 of 2.
 */
export function useListParams<TFilters extends Record<string, string>>(
  initialFilters: TFilters,
  options?: { perPage?: number; defaultSort?: string }
) {
  const [page, setPage] = React.useState(1);
  const [sort, setSortState] = React.useState(options?.defaultSort ?? '-created_at');
  const [filters, setFiltersState] = React.useState<TFilters>(initialFilters);

  const setFilter = React.useCallback((key: keyof TFilters, value: string) => {
    setFiltersState((current) => ({ ...current, [key]: value }));
    setPage(1);
  }, []);

  const setSort = React.useCallback((next: string) => {
    setSortState(next);
    setPage(1);
  }, []);

  const reset = React.useCallback(() => {
    setFiltersState(initialFilters);
    setPage(1);
    // `initialFilters` is a literal at every call site, so identity churn here
    // would reset on every render — compare by content instead.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(initialFilters)]);

  const params: ListParams = React.useMemo(
    () => ({ page, per_page: options?.perPage ?? 20, sort, ...filters }),
    [page, sort, filters, options?.perPage]
  );

  return { page, setPage, sort, setSort, filters, setFilter, reset, params };
}
