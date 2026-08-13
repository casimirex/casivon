import { Card, CardContent } from '@/components/ui/Card';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/Table';
import { formatMoney, getBaseCurrency } from '@/lib/utils';
import type { DocumentLine } from '@/types';

/** Key/value strip used at the top of every document detail page. */
export function SummaryGrid({
  items,
}: {
  items: Array<{ label: string; value: React.ReactNode }>;
}) {
  return (
    <dl className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
      {items.map((item) => (
        <Card key={item.label}>
          <CardContent className="pt-6">
            <dt className="text-xs font-medium uppercase tracking-wide text-slate-500">
              {item.label}
            </dt>
            <dd className="mt-1 text-lg font-semibold text-slate-900">{item.value}</dd>
          </CardContent>
        </Card>
      ))}
    </dl>
  );
}

/** Read-only line table with the document totals underneath. */
export function DocumentLines({
  lines,
  currency = 'USD',
  subtotal,
  tax,
  total,
  baseTotal,
  fxRate,
  extraRows,
}: {
  lines: DocumentLine[];
  currency?: string;
  subtotal?: string | null;
  tax?: string | null;
  total?: string | null;
  /** `total` restated in the base currency, as the server computed it. */
  baseTotal?: string | null;
  fxRate?: string | null;
  extraRows?: Array<{ label: string; value: React.ReactNode; emphasis?: boolean }>;
}) {
  // Only worth showing when the two differ: on a single-currency installation
  // the restated total is the total, and a second identical figure under it
  // reads as a mistake.
  const base = getBaseCurrency();
  const showBase = currency !== base && baseTotal != null;
  return (
    <div className="space-y-4">
      <Table>
        <TableHeader>
          <TableRow className="hover:bg-transparent">
            <TableHead>Description</TableHead>
            <TableHead className="text-right">Qty</TableHead>
            <TableHead className="text-right">Unit price</TableHead>
            <TableHead className="text-right">Disc %</TableHead>
            <TableHead className="text-right">Tax %</TableHead>
            <TableHead className="text-right">Line total</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {lines.length === 0 && (
            <TableRow>
              <TableCell colSpan={6} className="py-8 text-center text-slate-400">
                This document has no lines.
              </TableCell>
            </TableRow>
          )}
          {lines.map((line) => (
            <TableRow key={line.id}>
              <TableCell className="font-medium">{line.description}</TableCell>
              <TableCell className="text-right tabular-nums">{line.quantity}</TableCell>
              <TableCell className="text-right tabular-nums">
                {formatMoney(line.unit_price, currency)}
              </TableCell>
              <TableCell className="text-right tabular-nums">
                {Number(line.discount_percent).toFixed(2)}%
              </TableCell>
              <TableCell className="text-right tabular-nums">
                {Number(line.tax_rate).toFixed(2)}%
              </TableCell>
              <TableCell className="text-right font-medium tabular-nums">
                {formatMoney(line.line_total, currency)}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>

      <div className="flex justify-end">
        <dl className="w-full max-w-xs space-y-1 text-sm">
          <div className="flex justify-between">
            <dt className="text-slate-500">Subtotal</dt>
            <dd className="tabular-nums">{formatMoney(subtotal, currency)}</dd>
          </div>
          <div className="flex justify-between">
            <dt className="text-slate-500">Tax</dt>
            <dd className="tabular-nums">{formatMoney(tax, currency)}</dd>
          </div>
          <div className="flex justify-between border-t pt-1 text-base font-semibold">
            <dt>Total</dt>
            <dd className="tabular-nums">{formatMoney(total, currency)}</dd>
          </div>
          {showBase && (
            <div className="flex justify-between text-xs text-slate-500">
              <dt>
                In {base}
                {fxRate && ` at ${fxRate}`}
              </dt>
              <dd className="tabular-nums">{formatMoney(baseTotal, base)}</dd>
            </div>
          )}
          {extraRows?.map((row) => (
            <div
              key={row.label}
              className={
                row.emphasis
                  ? 'flex justify-between border-t pt-1 text-base font-semibold'
                  : 'flex justify-between'
              }
            >
              <dt className={row.emphasis ? undefined : 'text-slate-500'}>{row.label}</dt>
              <dd className="tabular-nums">{row.value}</dd>
            </div>
          ))}
        </dl>
      </div>
    </div>
  );
}
