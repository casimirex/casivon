import type { FieldValues, UseFormReturn } from 'react-hook-form';

/**
 * Mirrors the server's line maths (`shared::money::calculate_line`) so the
 * running total on screen matches what the API will store. The server stays
 * authoritative — this is a preview, not the source of truth.
 */
export function previewLine(
  quantity: number,
  unitPrice: number,
  discountPercent = 0,
  taxRate = 0
): { net: number; tax: number; gross: number } {
  const gross = (Number(quantity) || 0) * (Number(unitPrice) || 0);
  const discount = round(gross * ((Number(discountPercent) || 0) / 100));
  const net = round(gross - discount);
  const tax = round(net * ((Number(taxRate) || 0) / 100));
  return { net, tax, gross: round(net + tax) };
}

function round(value: number): number {
  return Math.round((value + Number.EPSILON) * 100) / 100;
}

export interface LineTotals {
  subtotal: number;
  tax: number;
  total: number;
}

export function sumLines(
  lines: Array<{
    quantity?: number | string;
    unit_price?: number | string;
    discount_percent?: number | string;
    tax_rate?: number | string;
  }>
): LineTotals {
  return lines.reduce<LineTotals>(
    (accumulator, line) => {
      const { net, tax } = previewLine(
        Number(line.quantity ?? 0),
        Number(line.unit_price ?? 0),
        Number(line.discount_percent ?? 0),
        Number(line.tax_rate ?? 0)
      );
      const subtotal = round(accumulator.subtotal + net);
      const taxTotal = round(accumulator.tax + tax);
      return { subtotal, tax: taxTotal, total: round(subtotal + taxTotal) };
    },
    { subtotal: 0, tax: 0, total: 0 }
  );
}

/**
 * Line-editing form shape. react-hook-form derives a literal union of field
 * paths from each schema, so a component reused across quote / order / invoice
 * / purchase-order forms cannot name them all. `LineFormValues` is the common
 * denominator; call sites hand over their form with `asLineForm`.
 */
export interface LineFormValues extends FieldValues {
  lines: Array<{
    product_id?: string;
    description: string;
    quantity: number;
    unit_price: number;
    discount_percent?: number;
    tax_rate?: number;
  }>;
}

/**
 * Bridges a concrete form into the shared editor. The runtime shape is
 * identical — only the compile-time field-path union differs — and each caller's
 * schema is checked to have a matching `lines` array before it gets here.
 */
export function asLineForm<T extends FieldValues>(
  form: UseFormReturn<T>
): UseFormReturn<LineFormValues> {
  return form as unknown as UseFormReturn<LineFormValues>;
}
