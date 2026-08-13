import { useState } from 'react';
import { Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card';
import { ConfirmDialog, Dialog } from '@/components/ui/Dialog';
import { Field, FormGrid } from '@/components/common/Field';
import { StatusBadge } from '@/components/ui/Badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/Table';
import { vendorPayments } from '@/hooks/usePurchasing';
import { cn, formatDate, formatMoney, getBaseCurrency, toDateInput } from '@/lib/utils';
import { PAYMENT_METHODS, type PurchaseOrderDetail, type VendorPayment } from '@/types';

/**
 * Money paid against a purchase order.
 *
 * The mirror of the payments panel on an invoice, and deliberately so: the two
 * are one idea pointing in opposite directions, and somebody who has used one
 * should not have to learn the other.
 */
export function VendorPaymentsPanel({ order }: { order: PurchaseOrderDetail }) {
  const payments = vendorPayments.useList({ po_id: order.id, per_page: 100 });
  const record = vendorPayments.useCreate();
  const remove = vendorPayments.useRemove();

  const [paying, setPaying] = useState(false);
  const [reversing, setReversing] = useState<VendorPayment | null>(null);

  const outstanding = Number(order.amount_due ?? 0);
  const rows = payments.data?.data ?? [];

  // A draft has not been committed to, so there is nothing owed on it yet —
  // the same rule the API enforces.
  const payable = order.status !== 'draft' && outstanding > 0;

  return (
    <Card>
      <CardHeader className="flex-row items-center justify-between space-y-0">
        <CardTitle className="text-base">Payments</CardTitle>
        {payable && (
          <Button size="sm" onClick={() => setPaying(true)}>
            Record payment
          </Button>
        )}
      </CardHeader>
      <CardContent>
        <dl className="mb-4 flex gap-8 text-sm">
          <div>
            <dt className="text-slate-500">Paid</dt>
            <dd className="font-medium tabular-nums">
              {formatMoney(order.amount_paid, order.currency)}
            </dd>
          </div>
          <div>
            <dt className="text-slate-500">Still owed</dt>
            <dd className="font-medium tabular-nums">
              {formatMoney(order.amount_due, order.currency)}
            </dd>
          </div>
        </dl>

        {rows.length === 0 ? (
          <p className="py-6 text-center text-sm text-slate-400">
            Nothing has been paid against this order yet.
          </p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className="hover:bg-transparent">
                <TableHead>Date</TableHead>
                <TableHead>Method</TableHead>
                <TableHead>Reference</TableHead>
                <TableHead className="text-right">Amount</TableHead>
                <TableHead />
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((payment) => (
                <TableRow key={payment.id}>
                  <TableCell>{formatDate(payment.payment_date)}</TableCell>
                  <TableCell>
                    <StatusBadge status={payment.payment_method} />
                  </TableCell>
                  <TableCell className="text-slate-500">{payment.reference ?? '—'}</TableCell>
                  <TableCell className="text-right font-medium tabular-nums">
                    {formatMoney(payment.amount, payment.currency)}
                    {/* Only when settling cost something other than what the
                        order committed to. Always zero in one currency. */}
                    {Number(payment.fx_gain_loss) !== 0 && (
                      <span
                        className={cn(
                          'block text-xs font-normal',
                          Number(payment.fx_gain_loss) > 0 ? 'text-emerald-600' : 'text-red-600'
                        )}
                      >
                        {Number(payment.fx_gain_loss) > 0 ? 'FX gain ' : 'FX loss '}
                        {formatMoney(Math.abs(Number(payment.fx_gain_loss)), getBaseCurrency())}
                      </span>
                    )}
                  </TableCell>
                  <TableCell className="text-right">
                    <button
                      onClick={() => setReversing(payment)}
                      className="rounded p-1.5 text-slate-400 hover:bg-red-50 hover:text-red-600"
                      aria-label="Reverse payment"
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>

      {paying && (
        <RecordVendorPaymentDialog
          order={order}
          outstanding={outstanding}
          busy={record.isPending}
          onClose={() => setPaying(false)}
          onSubmit={async (body) => {
            await record.mutateAsync(body);
            setPaying(false);
          }}
        />
      )}

      <ConfirmDialog
        open={reversing !== null}
        onClose={() => setReversing(null)}
        busy={remove.isPending}
        title="Reverse this payment?"
        message="The payment is removed and the order goes back to owing it. The ledger keeps both the original entries and their reversal, so the audit trail stays intact."
        confirmLabel="Reverse"
        onConfirm={async () => {
          if (reversing) await remove.mutateAsync(reversing.id);
          setReversing(null);
        }}
      />
    </Card>
  );
}

function RecordVendorPaymentDialog({
  order,
  outstanding,
  busy,
  onClose,
  onSubmit,
}: {
  order: PurchaseOrderDetail;
  outstanding: number;
  busy: boolean;
  onClose: () => void;
  onSubmit: (body: Record<string, unknown>) => Promise<void>;
}) {
  const [amount, setAmount] = useState(outstanding.toFixed(2));
  const [method, setMethod] = useState('bank_transfer');
  const [date, setDate] = useState(toDateInput(new Date().toISOString()));
  const [reference, setReference] = useState('');

  const over = Number(amount) > outstanding;

  return (
    <Dialog
      open
      onClose={onClose}
      title={`Record a payment for ${order.po_number}`}
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button
            disabled={busy || over || !(Number(amount) > 0)}
            onClick={() =>
              onSubmit({
                po_id: order.id,
                amount: Number(amount).toFixed(2),
                payment_method: method,
                payment_date: date,
                reference: reference || undefined,
              })
            }
          >
            {busy ? 'Recording…' : 'Record payment'}
          </Button>
        </>
      }
    >
      <FormGrid>
        <Field
          label="Amount"
          required
          error={over ? `Only ${formatMoney(outstanding, order.currency)} is outstanding` : undefined}
          hint={`Outstanding: ${formatMoney(outstanding, order.currency)}`}
        >
          <Input
            type="number"
            step="0.01"
            min={0}
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            aria-label="Amount"
          />
        </Field>
        <Field label="Payment date" required>
          <Input
            type="date"
            value={date}
            onChange={(e) => setDate(e.target.value)}
            aria-label="Payment date"
          />
        </Field>
        <Field label="Method" required>
          <Select
            options={PAYMENT_METHODS}
            value={method}
            onChange={(e) => setMethod(e.target.value)}
            aria-label="Method"
          />
        </Field>
        <Field label="Reference" hint="Bank reference or cheque number">
          <Input
            value={reference}
            onChange={(e) => setReference(e.target.value)}
            aria-label="Reference"
          />
        </Field>
      </FormGrid>
    </Dialog>
  );
}
