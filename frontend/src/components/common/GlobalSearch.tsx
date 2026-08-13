import { useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { Loader2, Search } from 'lucide-react';
import { cn } from '@/lib/utils';
import { useDebounced } from '@/hooks/useDebounced';
import { MIN_SEARCH_LENGTH, useSearch } from '@/hooks/useSearch';
import type { SearchHit } from '@/types';

/**
 * Where each kind lives, and what to call it.
 *
 * The API returns `kind` and `id` and no URL — route shapes are the frontend's
 * business, so this table is the only place that knows them. Kinds absent here
 * simply do not render, which is what happens if the backend gains one before
 * this does.
 */
const DESTINATIONS: Record<string, { label: string; path: (id: string) => string }> = {
  contact: { label: 'Contacts', path: () => '/crm' },
  company: { label: 'Companies', path: () => '/crm' },
  opportunity: { label: 'Opportunities', path: () => '/crm' },
  quote: { label: 'Quotes', path: (id) => `/sales/quotes/${id}` },
  order: { label: 'Orders', path: (id) => `/sales/orders/${id}` },
  invoice: { label: 'Invoices', path: (id) => `/sales/invoices/${id}` },
  product: { label: 'Products', path: (id) => `/inventory/products/${id}` },
  warehouse: { label: 'Warehouses', path: () => '/inventory/warehouses' },
  vendor: { label: 'Vendors', path: () => '/purchasing/vendors' },
  purchase_order: {
    label: 'Purchase orders',
    path: (id) => `/purchasing/purchase-orders/${id}`,
  },
  project: { label: 'Projects', path: (id) => `/projects/${id}` },
  task: { label: 'Tasks', path: () => '/projects' },
  account: { label: 'Accounts', path: () => '/accounting/accounts' },
  ledger_entry: { label: 'Ledger entries', path: () => '/accounting/ledger' },
  employee: { label: 'Employees', path: (id) => `/hr/employees/${id}` },
};

/** Groups hits by kind, keeping the server's order within each group. */
function grouped(hits: SearchHit[]): Array<[string, SearchHit[]]> {
  const groups = new Map<string, SearchHit[]>();
  for (const hit of hits) {
    if (!DESTINATIONS[hit.kind]) continue;
    const existing = groups.get(hit.kind);
    if (existing) existing.push(hit);
    else groups.set(hit.kind, [hit]);
  }
  return [...groups.entries()];
}

export function GlobalSearch() {
  const [term, setTerm] = useState('');
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const container = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();

  const debounced = useDebounced(term);
  const search = useSearch(debounced);

  // Flat, in render order, so the arrow keys move through what is on screen.
  const groups = useMemo(() => grouped(search.data?.hits ?? []), [search.data]);
  const flat = useMemo(() => groups.flatMap(([, hits]) => hits), [groups]);

  useEffect(() => setActive(0), [debounced]);

  // ⌘K from anywhere, and Esc to leave.
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        input.current?.focus();
        setOpen(true);
      }
      if (event.key === 'Escape') setOpen(false);
    }
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  // Clicking anywhere else dismisses the results, as a dropdown should.
  useEffect(() => {
    function onClick(event: MouseEvent) {
      if (!container.current?.contains(event.target as Node)) setOpen(false);
    }
    document.addEventListener('mousedown', onClick);
    return () => document.removeEventListener('mousedown', onClick);
  }, []);

  function go(hit: SearchHit) {
    navigate(DESTINATIONS[hit.kind].path(hit.id));
    setOpen(false);
    setTerm('');
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setActive((n) => Math.min(n + 1, flat.length - 1));
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      setActive((n) => Math.max(n - 1, 0));
    } else if (event.key === 'Enter' && flat[active]) {
      event.preventDefault();
      go(flat[active]);
    }
  }

  const tooShort = debounced.trim().length > 0 && debounced.trim().length < MIN_SEARCH_LENGTH;
  const showResults = open && debounced.trim().length >= MIN_SEARCH_LENGTH;

  // Tracks position across groups so the highlight matches the arrow keys.
  let index = -1;

  return (
    <div ref={container} className="relative max-w-md flex-1">
      <div className="relative">
        <Search
          className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-400"
          aria-hidden
        />
        <input
          ref={input}
          type="search"
          value={term}
          onChange={(e) => {
            setTerm(e.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onKeyDown={onKeyDown}
          placeholder="Search everything…"
          aria-label="Search"
          role="combobox"
          aria-expanded={showResults}
          aria-controls="global-search-results"
          className="h-9 w-full rounded-md border border-slate-200 bg-slate-50 pl-9 pr-10 text-sm placeholder:text-slate-400 focus:border-slate-300 focus:bg-white focus:outline-none focus:ring-2 focus:ring-slate-200"
        />
        {search.isFetching ? (
          <Loader2
            className="absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 animate-spin text-slate-400"
            aria-hidden
          />
        ) : (
          <kbd className="pointer-events-none absolute right-3 top-1/2 hidden -translate-y-1/2 rounded border border-slate-200 bg-white px-1.5 py-0.5 text-[10px] font-medium text-slate-400 sm:block">
            ⌘K
          </kbd>
        )}
      </div>

      {tooShort && open && (
        <p className="absolute z-30 mt-1 w-full rounded-md border bg-white px-3 py-2 text-xs text-slate-500 shadow-lg">
          Keep typing — searching needs at least {MIN_SEARCH_LENGTH} characters.
        </p>
      )}

      {showResults && (
        <div
          id="global-search-results"
          role="listbox"
          className="absolute z-30 mt-1 max-h-96 w-full overflow-y-auto rounded-md border bg-white py-1 shadow-lg"
        >
          {flat.length === 0 && !search.isFetching && (
            <p className="px-3 py-6 text-center text-sm text-slate-400">
              Nothing matches “{debounced}”.
            </p>
          )}

          {groups.map(([kind, hits]) => (
            <div key={kind}>
              <p className="px-3 pb-1 pt-2 text-xs font-semibold uppercase tracking-wide text-slate-400">
                {DESTINATIONS[kind].label}
              </p>
              {hits.map((hit) => {
                index += 1;
                const highlighted = index === active;
                return (
                  <button
                    key={`${hit.kind}-${hit.id}`}
                    role="option"
                    aria-selected={highlighted}
                    onClick={() => go(hit)}
                    onMouseEnter={() => setActive(flat.indexOf(hit))}
                    className={cn(
                      'flex w-full items-baseline gap-2 px-3 py-1.5 text-left text-sm',
                      highlighted ? 'bg-slate-100' : 'hover:bg-slate-50'
                    )}
                  >
                    <span className="truncate font-medium text-slate-900">{hit.title}</span>
                    {hit.subtitle && (
                      <span className="truncate text-xs text-slate-500">{hit.subtitle}</span>
                    )}
                  </button>
                );
              })}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
