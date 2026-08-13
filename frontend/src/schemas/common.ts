import { z } from 'zod';

/**
 * Shared Zod building blocks. Each rule mirrors a `validator` rule on the Rust
 * DTO, so the browser rejects what the server would reject — the server stays
 * the authority, this just saves a round trip and points at the right field.
 */

/** Text inputs hand back `''`, which should mean "not provided", not "empty". */
export const optionalString = z
  .string()
  .trim()
  .optional()
  .transform((value) => (value === '' ? undefined : value));

export const requiredString = (label: string, max = 255) =>
  z.string().trim().min(1, `${label} is required`).max(max, `${label} must be ${max} characters or fewer`);

export const optionalEmail = z
  .union([z.literal(''), z.string().trim().email('Enter a valid email address')])
  .optional()
  .transform((value) => (value === '' ? undefined : value));

export const requiredEmail = z.string().trim().min(1, 'Email is required').email('Enter a valid email address');

export const optionalUrl = z
  .union([z.literal(''), z.string().trim().url('Enter a valid URL, including https://')])
  .optional()
  .transform((value) => (value === '' ? undefined : value));

/** Selects submit `''` for "nothing chosen"; the API wants the key omitted. */
export const optionalUuid = z
  .string()
  .optional()
  .transform((value) => (value === '' ? undefined : value));

export const requiredUuid = (label: string) =>
  z.string().min(1, `${label} is required`).uuid(`${label} is invalid`);

export const isoDate = (label: string) =>
  z.string().min(1, `${label} is required`).regex(/^\d{4}-\d{2}-\d{2}$/, `${label} must be a date`);

export const optionalIsoDate = z
  .union([z.literal(''), z.string().regex(/^\d{4}-\d{2}-\d{2}$/, 'Must be a date')])
  .optional()
  .transform((value) => (value === '' ? undefined : value));

/**
 * Money as typed into a number input. Kept as a number through validation so
 * `min`/`max` work, then serialised to a fixed-scale string on submit — the API
 * takes DECIMAL as a string to avoid float drift.
 */
export const money = (label: string, { min = 0 }: { min?: number } = {}) =>
  z.coerce
    .number({ invalid_type_error: `${label} must be a number` })
    .min(min, min === 0 ? `${label} cannot be negative` : `${label} must be at least ${min}`)
    .max(999_999_999, `${label} is too large`);

export const optionalMoney = (label: string) =>
  z
    .union([z.literal(''), z.coerce.number({ invalid_type_error: `${label} must be a number` }).min(0)])
    .optional()
    .transform((value) => (value === '' || value === undefined ? undefined : Number(value)));

export const positiveInt = (label: string) =>
  z.coerce
    .number({ invalid_type_error: `${label} must be a number` })
    .int(`${label} must be a whole number`)
    .min(1, `${label} must be at least 1`);

/** A field that may be left blank but is a percentage when filled. */
export const optionalPercent = (label: string) =>
  z
    .union([z.literal(''), percent(label)])
    .optional()
    .transform((value) => (value === '' || value === undefined ? undefined : Number(value)));

export const percent = (label: string) =>
  z.coerce
    .number({ invalid_type_error: `${label} must be a number` })
    .min(0, `${label} cannot be negative`)
    .max(100, `${label} cannot exceed 100`);

export const currencyCode = z
  .string()
  .trim()
  .length(3, 'Use a 3-letter currency code, e.g. USD')
  .toUpperCase();

export const optionalCurrency = z
  .union([z.literal(''), currencyCode])
  .optional()
  .transform((value) => (value === '' ? undefined : value));

/** Narrows a `readonly string[]` of allowed values into a Zod enum. */
export function enumOf<T extends readonly [string, ...string[]]>(values: T, label: string) {
  return z.enum(values, {
    errorMap: () => ({ message: `Choose a valid ${label}` }),
  });
}

/**
 * Serialises a validated form into an API payload: money fields become
 * fixed-scale strings and `undefined` keys are dropped so a PUT does not blank
 * out fields the user never touched.
 */
export function toPayload<T extends Record<string, unknown>>(
  values: T,
  moneyFields: readonly (keyof T)[] = []
): Record<string, unknown> {
  const payload: Record<string, unknown> = {};

  for (const [key, value] of Object.entries(values)) {
    if (value === undefined || value === '') continue;

    payload[key] =
      moneyFields.includes(key as keyof T) && typeof value === 'number'
        ? value.toFixed(2)
        : value;
  }

  return payload;
}
