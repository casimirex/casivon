import * as React from 'react';
import { Search, X } from 'lucide-react';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { Button } from '@/components/ui/Button';

/**
 * Debounced search box. Typing should not fire a request per keystroke, but the
 * input itself must stay responsive, so the visible value is local state.
 */
export function SearchInput({
  value,
  onChange,
  placeholder = 'Search…',
  delay = 300,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  delay?: number;
}) {
  const [draft, setDraft] = React.useState(value);

  // Keep in step when the parent resets filters.
  React.useEffect(() => setDraft(value), [value]);

  React.useEffect(() => {
    if (draft === value) return;
    const timer = window.setTimeout(() => onChange(draft), delay);
    return () => window.clearTimeout(timer);
  }, [draft, delay, onChange, value]);

  return (
    <div className="relative flex-1 sm:max-w-xs">
      <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-400" />
      <Input
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        placeholder={placeholder}
        className="pl-9 pr-9"
        aria-label={placeholder}
      />
      {draft && (
        <button
          type="button"
          onClick={() => setDraft('')}
          className="absolute right-3 top-1/2 -translate-y-1/2 text-slate-400 hover:text-slate-600"
          aria-label="Clear search"
        >
          <X className="h-4 w-4" />
        </button>
      )}
    </div>
  );
}

export interface FilterBarProps {
  search?: { value: string; onChange: (value: string) => void; placeholder?: string };
  selects?: Array<{
    label: string;
    value: string;
    onChange: (value: string) => void;
    options: readonly string[] | readonly { value: string; label: string }[];
  }>;
  onReset?: () => void;
  children?: React.ReactNode;
}

export function FilterBar({ search, selects, onReset, children }: FilterBarProps) {
  const hasActiveFilter =
    Boolean(search?.value) || Boolean(selects?.some((select) => select.value));

  return (
    <div className="flex flex-wrap items-center gap-3">
      {search && (
        <SearchInput
          value={search.value}
          onChange={search.onChange}
          placeholder={search.placeholder}
        />
      )}
      {selects?.map((select) => (
        <Select
          key={select.label}
          value={select.value}
          onChange={(event) => select.onChange(event.target.value)}
          options={select.options}
          placeholder={`All ${select.label.toLowerCase()}`}
          className="w-auto min-w-[10rem]"
          aria-label={select.label}
        />
      ))}
      {children}
      {onReset && hasActiveFilter && (
        <Button variant="ghost" size="sm" onClick={onReset}>
          <X className="mr-1 h-4 w-4" />
          Clear filters
        </Button>
      )}
    </div>
  );
}
