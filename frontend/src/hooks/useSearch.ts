import { useQuery } from '@tanstack/react-query';
import { http } from '@/api/client';
import type { SearchResults } from '@/types';

/** Below this the server returns nothing, so there is no point asking. */
export const MIN_SEARCH_LENGTH = 2;

/**
 * Results for a term across every module the signed-in user may see.
 *
 * What comes back is already scoped to their roles — an accountant and a
 * salesperson get different answers — so nothing needs hiding here.
 */
export function useSearch(term: string) {
  const enabled = term.trim().length >= MIN_SEARCH_LENGTH;

  return useQuery({
    queryKey: ['search', term],
    queryFn: () => http.get<SearchResults>(`/search?q=${encodeURIComponent(term)}`),
    enabled,
    // A term the user already typed gives the same answer for a while; this
    // makes backspacing through a word feel instant rather than re-querying.
    staleTime: 30_000,
  });
}
