import { type ClassValue, clsx } from 'clsx';
import { twMerge } from 'tailwind-merge';
import { format, parseISO } from 'date-fns';

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/**
 * The organisation's currency, for the call sites that do not carry one.
 *
 * A module-level value rather than a hook because `formatMoney` is a plain
 * function called from ~60 places, most of which have no access to React
 * context. `CurrencyProvider` sets it as soon as the organisation profile
 * loads; until then it falls back to USD, which is the seeded default.
 */
let baseCurrency = 'USD';

export function setBaseCurrency(code: string) {
  if (code) baseCurrency = code.toUpperCase();
}

export function getBaseCurrency() {
  return baseCurrency;
}

/**
 * Money arrives from the API as a string so no precision is lost in JSON.
 * Format for display only — never parse it back and send the float onward.
 *
 * `currency` defaults to the organisation's, not to USD: there is exactly one
 * currency in the system, and hardcoding a different one here would mislabel
 * every amount on the screen.
 */
export function formatMoney(
  value: string | number | null | undefined,
  currency = baseCurrency
): string {
  if (value === null || value === undefined || value === '') return '—';
  const amount = typeof value === 'string' ? Number(value) : value;
  if (Number.isNaN(amount)) return '—';

  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency,
    minimumFractionDigits: 2,
  }).format(amount);
}

/** Sends a form's numeric money field back as a fixed-scale string. */
export function toMoneyString(value: number | string | null | undefined): string | undefined {
  if (value === null || value === undefined || value === '') return undefined;
  const amount = typeof value === 'string' ? Number(value) : value;
  return Number.isNaN(amount) ? undefined : amount.toFixed(2);
}

export function formatDate(value: string | null | undefined, pattern = 'dd MMM yyyy'): string {
  if (!value) return '—';
  try {
    return format(parseISO(value), pattern);
  } catch {
    return '—';
  }
}

export function formatDateTime(value: string | null | undefined): string {
  return formatDate(value, 'dd MMM yyyy HH:mm');
}

/** `2026-08-09` — the shape every `<input type="date">` and the API expect. */
export function toDateInput(value: Date | string = new Date()): string {
  const date = typeof value === 'string' ? parseISO(value) : value;
  return format(date, 'yyyy-MM-dd');
}

export function formatNumber(value: number | string | null | undefined): string {
  if (value === null || value === undefined || value === '') return '—';
  const amount = typeof value === 'string' ? Number(value) : value;
  return Number.isNaN(amount) ? '—' : new Intl.NumberFormat('en-US').format(amount);
}

/** `partially_received` -> `Partially received` */
export function humanize(value: string | null | undefined): string {
  if (!value) return '—';
  const spaced = value.replace(/_/g, ' ');
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

export function initials(first?: string, last?: string): string {
  return `${first?.[0] ?? ''}${last?.[0] ?? ''}`.toUpperCase() || '?';
}

/**
 * Drops keys the user left empty so PATCH-style updates don't blank out fields
 * that were simply not touched.
 */
export function pruneEmpty<T extends Record<string, unknown>>(input: T): Partial<T> {
  return Object.fromEntries(
    Object.entries(input).filter(([, value]) => value !== '' && value !== undefined && value !== null)
  ) as Partial<T>;
}
