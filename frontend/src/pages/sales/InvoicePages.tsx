import { useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { CreditCard, FileMinus2, Trash2 } from 'lucide-react';
import { PageHeader } from '@/components/common/PageHeader';
import { DataTable, type Column } from '@/components/common/DataTable';
import { FilterBar } from '@/components/common/Filters';
import { DocumentLines, SummaryGrid } from '@/components/common/DocumentView';
import { Field, FormGrid } from '@/components/common/Field';
import { DetailSkeleton } from '@/components/ui/Skeleton';
import { ErrorState } from '@/components/common/States';
import { Button } from '@/components/ui/Button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { Textarea } from '@/components/ui/Textarea';
import { Dialog, ConfirmDialog } from '@/components/ui/Dialog';
import { StatusBadge } from '@/components/ui/Badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/Table';
import { useListParams } from '@/hooks/useListParams';
import {
  creditNotes,
  invoices,
  payments,
  useCreateCreditNote,
  useDeletePayment,
  useInvoiceStatus,
  useRecordPayment,
} from '@/hooks/useSales';
import { useWarehouseOptions } from '@/hooks/useInventory';
import { cn, formatDate, formatMoney, getBaseCurrency, toDateInput } from '@/lib/utils';
import { creditNoteSchema, paymentSchema, type PaymentForm } from '@/schemas';
import {
  INVOICE_STATUSES,
  PAYMENT_METHODS,
  type CreditNote,
  type Invoice,
  type Payment,
} from '@/types';

export function InvoiceList() {
  const navigate = useNavigate();
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams({
    search: '',
    status: '',
  });
  const query = invoices.useList(params);

  const columns: Column<Invoice>[] = [
    {
      key: 'invoice_number',
      header: 'Invoice',
      render: (row) => <span className="font-medium text-slate-900">{row.invoice_number}</span>,
    },
    { key: 'status', header: 'Status', render: (row) => <StatusBadge status={row.status} /> },
    { key: 'issue_date', header: 'Issued', sortable: true, render: (row) => formatDate(row.issue_date) },
    { key: 'due_date', header: 'Due', sortable: true, render: (row) => formatDate(row.due_date) },
    {
      key: 'total',
      header: 'Total',
      sortable: true,
      align: 'right',
      render: (row) => <span className="tabular-nums">{formatMoney(row.total, row.currency)}</span>,
    },
    {
      key: 'amount_due',
      header: 'Outstanding',
      sortable: true,
      align: 'right',
      render: (row) => (
        <span
          className={`font-medium tabular-nums ${
            Number(row.amount_due ?? 0) > 0 ? 'text-slate-900' : 'text-green-600'
          }`}
        >
          {formatMoney(row.amount_due, row.currency)}
        </span>
      ),
    },
  ];

  return (
    <div className="space-y-6">
      <PageHeader title="Invoices" description="What customers owe, and what they have settled" />

      <FilterBar
        search={{
          value: filters.search,
          onChange: (value) => setFilter('search', value),
          placeholder: 'Search by invoice number…',
        }}
        selects={[
          {
            label: 'Status',
            value: filters.status,
            onChange: (value) => setFilter('status', value),
            options: INVOICE_STATUSES,
          },
        ]}
        onReset={reset}
      />

      <DataTable
        columns={columns}
        rows={query.data?.data}
        rowKey={(row) => row.id}
        isLoading={query.isLoading}
        error={query.error}
        onRetry={query.refetch}
        onRowClick={(row) => navigate(`/sales/invoices/${row.id}`)}
        pagination={query.data?.pagination}
        onPageChange={setPage}
        sort={sort}
        onSortChange={setSort}
        emptyTitle="No invoices"
        emptyMessage="Convert a confirmed sales order to raise the first invoice."
        emptyAction={<Button onClick={() => navigate('/sales/orders')}>Go to orders</Button>}
      />
    </div>
  );
}

const NEXT_STATUS: Record<string, Array<{ status: string; label: string }>> = {
  draft: [
    { status: 'sent', label: 'Send to customer' },
    { status: 'cancelled', label: 'Cancel' },
  ],
  sent: [{ status: 'cancelled', label: 'Cancel' }],
  overdue: [{ status: 'cancelled', label: 'Cancel' }],
};

export function InvoiceDetail() {
  const { id } = useParams<{ id: string }>();
  const query = invoices.useOne(id);
  const setStatus = useInvoiceStatus();
  const deletePayment = useDeletePayment();
  const [paying, setPaying] = useState(false);
  const [crediting, setCrediting] = useState(false);
  const [reversing, setReversing] = useState<Payment | null>(null);

  if (query.isLoading) return <DetailSkeleton />;
  if (query.error) return <ErrorState error={query.error} onRetry={query.refetch} />;
  if (!query.data) return null;

  const invoice = query.data;
  const outstanding = Number(invoice.amount_due ?? 0);
  const transitions = NEXT_STATUS[invoice.status] ?? [];
  // The server refuses payments on anything but a live receivable: a draft has
  // raised nothing to settle, a paid one has nothing outstanding, and a
  // cancelled one is closed.
  const canPay = !['draft', 'cancelled', 'paid'].includes(invoice.status) && outstanding > 0;
  // Deliberately wider than `canPay`, and this is the whole point: a *paid*
  // invoice has no status transition left, so crediting was the one thing that
  // could not be done to it.
  const canCredit = !['draft', 'cancelled'].includes(invoice.status);

  return (
    <div className="space-y-6">
      <PageHeader
        title={invoice.invoice_number}
        backTo="/sales/invoices"
        backLabel="Back to invoices"
        badge={<StatusBadge status={invoice.status} />}
        actions={
          <>
            {transitions.map((transition) => (
              <Button
                key={transition.status}
                variant="outline"
                disabled={setStatus.isPending}
                onClick={() => setStatus.mutate({ id: invoice.id, status: transition.status })}
              >
                {transition.label}
              </Button>
            ))}
            {canCredit && (
              <Button variant="outline" onClick={() => setCrediting(true)}>
                <FileMinus2 className="mr-1 h-4 w-4" />
                Credit note
              </Button>
            )}
            {canPay && (
              <Button onClick={() => setPaying(true)}>
                <CreditCard className="mr-1 h-4 w-4" />
                Record payment
              </Button>
            )}
          </>
        }
      />

      <SummaryGrid
        items={[
          { label: 'Issued', value: formatDate(invoice.issue_date) },
          { label: 'Due', value: formatDate(invoice.due_date) },
          { label: 'Paid', value: formatMoney(invoice.amount_paid, invoice.currency) },
          {
            label: 'Outstanding',
            value: (
              <span className={outstanding > 0 ? 'text-slate-900' : 'text-green-600'}>
                {formatMoney(invoice.amount_due, invoice.currency)}
              </span>
            ),
          },
        ]}
      />

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Line items</CardTitle>
        </CardHeader>
        <CardContent>
          <DocumentLines
            lines={invoice.lines}
            currency={invoice.currency}
            subtotal={invoice.subtotal}
            tax={invoice.tax_amount}
            total={invoice.total}
            baseTotal={invoice.base_total}
            fxRate={invoice.fx_rate}
            extraRows={[
              {
                label: 'Paid',
                value: `− ${formatMoney(invoice.amount_paid, invoice.currency)}`,
              },
              {
                label: 'Outstanding',
                value: formatMoney(invoice.amount_due, invoice.currency),
                emphasis: true,
              },
            ]}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Payments</CardTitle>
        </CardHeader>
        <CardContent>
          {invoice.payments.length === 0 ? (
            <p className="py-6 text-center text-sm text-slate-400">
              Nothing has been paid against this invoice yet.
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
                {invoice.payments.map((payment) => (
                  <TableRow key={payment.id}>
                    <TableCell>{formatDate(payment.payment_date)}</TableCell>
                    <TableCell>
                      <StatusBadge status={payment.payment_method} />
                    </TableCell>
                    <TableCell className="text-slate-500">{payment.reference ?? '—'}</TableCell>
                    <TableCell className="text-right font-medium tabular-nums">
                      {formatMoney(payment.amount, payment.currency)}
                      {/* Only when the money turned out to be worth something
                          other than the invoice booked it at. On a
                          single-currency installation this is always zero. */}
                      {Number(payment.fx_gain_loss) !== 0 && (
                        <span
                          className={cn(
                            'block text-xs font-normal',
                            Number(payment.fx_gain_loss) > 0 ? 'text-emerald-600' : 'text-red-600'
                          )}
                        >
                          {Number(payment.fx_gain_loss) > 0 ? 'FX gain ' : 'FX loss '}
                          {formatMoney(
                            Math.abs(Number(payment.fx_gain_loss)),
                            getBaseCurrency()
                          )}
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
      </Card>

      {paying && (
        <RecordPaymentDialog
          invoiceId={invoice.id}
          currency={invoice.currency}
          outstanding={outstanding}
          onClose={() => setPaying(false)}
        />
      )}
      {crediting && (
        <CreditNoteDialog
          invoiceId={invoice.id}
          lines={invoice.lines.map((line) => ({
            invoice_line_id: line.id,
            description: line.description,
            creditable: line.quantity,
          }))}
          onClose={() => setCrediting(false)}
        />
      )}

      <ConfirmDialog
        open={Boolean(reversing)}
        onClose={() => setReversing(null)}
        onConfirm={() => {
          if (reversing) deletePayment.mutate(reversing.id, { onSuccess: () => setReversing(null) });
        }}
        title="Reverse payment"
        message={`Remove the ${formatMoney(reversing?.amount, invoice.currency)} payment? The invoice balance will be recalculated.`}
        confirmLabel="Reverse payment"
        busy={deletePayment.isPending}
      />
    </div>
  );
}

function RecordPaymentDialog({
  invoiceId,
  currency,
  outstanding,
  onClose,
}: {
  invoiceId: string;
  currency: string;
  outstanding: number;
  onClose: () => void;
}) {
  const record = useRecordPayment();

  const form = useForm<PaymentForm>({
    // The schema caps the amount at what is still outstanding, exactly as the
    // server does, so the user sees the problem before submitting.
    resolver: zodResolver(paymentSchema(outstanding)),
    defaultValues: {
      amount: outstanding,
      payment_method: 'bank_transfer',
      payment_date: toDateInput(),
      reference: '',
      notes: '',
    },
  });

  const { errors } = form.formState;

  const onSubmit = form.handleSubmit((values) =>
    record.mutate(
      {
        invoice_id: invoiceId,
        amount: Number(values.amount).toFixed(2),
        payment_method: values.payment_method,
        payment_date: values.payment_date,
        reference: values.reference || undefined,
        notes: values.notes || undefined,
      },
      { onSuccess: onClose }
    )
  );

  return (
    <Dialog
      open
      onClose={onClose}
      title="Record payment"
      description={`${formatMoney(outstanding, currency)} outstanding`}
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={record.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={record.isPending}>
            {record.isPending ? 'Recording…' : 'Record payment'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <FormGrid>
          <Field label="Amount" required error={errors.amount?.message}>
            <Input type="number" step="0.01" min={0.01} {...form.register('amount')} />
          </Field>
          <Field label="Method" required error={errors.payment_method?.message}>
            <Select options={PAYMENT_METHODS} {...form.register('payment_method')} />
          </Field>
          <Field label="Payment date" required error={errors.payment_date?.message}>
            <Input type="date" {...form.register('payment_date')} />
          </Field>
          <Field label="Reference" error={errors.reference?.message}>
            <Input placeholder="Bank reference" {...form.register('reference')} />
          </Field>
        </FormGrid>
        <Field label="Notes" error={errors.notes?.message}>
          <Textarea {...form.register('notes')} />
        </Field>
      </form>
    </Dialog>
  );
}

export function PaymentList() {
  const navigate = useNavigate();
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams(
    { payment_method: '' },
    { defaultSort: '-payment_date' }
  );
  const query = payments.useList(params);

  const columns: Column<Payment>[] = [
    {
      key: 'payment_date',
      header: 'Date',
      sortable: true,
      render: (row) => formatDate(row.payment_date),
    },
    {
      key: 'payment_method',
      header: 'Method',
      render: (row) => <StatusBadge status={row.payment_method} />,
    },
    { key: 'reference', header: 'Reference', render: (row) => row.reference ?? '—' },
    {
      key: 'amount',
      header: 'Amount',
      sortable: true,
      align: 'right',
      render: (row) => (
        <span className="font-medium tabular-nums">{formatMoney(row.amount, row.currency)}</span>
      ),
    },
  ];

  return (
    <div className="space-y-6">
      <PageHeader title="Payments" description="Every payment received, newest first" />

      <FilterBar
        selects={[
          {
            label: 'Method',
            value: filters.payment_method,
            onChange: (value) => setFilter('payment_method', value),
            options: PAYMENT_METHODS,
          },
        ]}
        onReset={reset}
      />

      <DataTable
        columns={columns}
        rows={query.data?.data}
        rowKey={(row) => row.id}
        isLoading={query.isLoading}
        error={query.error}
        onRetry={query.refetch}
        onRowClick={(row) => navigate(`/sales/invoices/${row.invoice_id}`)}
        pagination={query.data?.pagination}
        onPageChange={setPage}
        sort={sort}
        onSortChange={setSort}
        emptyTitle="No payments recorded"
        emptyMessage="Payments recorded against invoices show up here."
      />
    </div>
  );
}


/**
 * Crediting a customer against an invoice.
 *
 * Quantities start at **zero**, like the purchase return and unlike receiving
 * goods: crediting a whole invoice is not the common case, and a form that
 * defaults to it is one stray click from giving away the sale.
 *
 * The warehouse is optional on purpose. Naming one brings the goods back onto
 * the shelf; leaving it empty credits money only, which is what a price dispute
 * or an over-billing needs.
 */
function CreditNoteDialog({
  invoiceId,
  lines,
  onClose,
}: {
  invoiceId: string;
  lines: Array<{ invoice_line_id: string; description: string; creditable: number }>;
  onClose: () => void;
}) {
  const issue = useCreateCreditNote();
  const { options: warehouseOptions } = useWarehouseOptions();

  const form = useForm({
    resolver: zodResolver(creditNoteSchema),
    defaultValues: {
      warehouse_id: '',
      issue_date: toDateInput(),
      reason: '',
      notes: '',
      lines: lines.map((line) => ({ ...line, quantity: 0 })),
    },
  });

  const { errors } = form.formState;

  const onSubmit = form.handleSubmit((values) =>
    issue.mutate(
      {
        invoice_id: invoiceId,
        warehouse_id: values.warehouse_id || undefined,
        issue_date: values.issue_date || undefined,
        reason: values.reason || undefined,
        notes: values.notes || undefined,
        lines: values.lines
          .filter((line) => Number(line.quantity) > 0)
          .map((line) => ({
            invoice_line_id: line.invoice_line_id,
            quantity: Number(line.quantity),
          })),
      },
      { onSuccess: onClose }
    )
  );

  return (
    <Dialog
      open
      onClose={onClose}
      title="Issue a credit note"
      description="Reduces what this invoice is owed. Name a warehouse only if the goods came back."
      className="max-w-2xl"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={issue.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={issue.isPending}>
            {issue.isPending ? 'Issuing…' : 'Issue credit note'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <FormGrid>
          <Field label="Issue date" error={errors.issue_date?.message}>
            <Input type="date" {...form.register('issue_date')} />
          </Field>
          <Field
            label="Warehouse"
            hint="Only if the goods came back — leave empty to credit money alone"
            error={errors.warehouse_id?.message}
          >
            {/* `Field` renders the label but does not associate it with the
                control, so without this the select has no accessible name. */}
            <Select
              options={[{ value: '', label: 'No goods returned' }, ...warehouseOptions]}
              aria-label="Warehouse"
              {...form.register('warehouse_id')}
            />
          </Field>
        </FormGrid>

        <Field label="Reason" error={errors.reason?.message}>
          <Input placeholder="Returned by the customer, agreed discount…" {...form.register('reason')} />
        </Field>

        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-slate-900">Quantities to credit</h3>
          <div className="overflow-hidden rounded-md border">
            <table className="w-full text-sm">
              <thead className="border-b bg-slate-50">
                <tr>
                  <th className="px-3 py-2 text-left text-xs font-medium uppercase text-slate-500">
                    Item
                  </th>
                  <th className="w-28 px-3 py-2 text-right text-xs font-medium uppercase text-slate-500">
                    Invoiced
                  </th>
                  <th className="w-32 px-3 py-2 text-right text-xs font-medium uppercase text-slate-500">
                    Crediting
                  </th>
                </tr>
              </thead>
              <tbody>
                {lines.map((line, index) => {
                  const rowError = (errors.lines ?? [])[index] as
                    | Record<string, { message?: string }>
                    | undefined;

                  return (
                    <tr key={line.invoice_line_id} className="border-b last:border-0">
                      <td className="px-3 py-2">{line.description}</td>
                      <td className="px-3 py-2 text-right tabular-nums text-slate-500">
                        {line.creditable}
                      </td>
                      <td className="px-3 py-2">
                        <Input
                          type="number"
                          min={0}
                          max={line.creditable}
                          className="text-right"
                          {...form.register(`lines.${index}.quantity`)}
                        />
                        {rowError?.quantity?.message && (
                          <p className="mt-1 text-xs text-red-600">{rowError.quantity.message}</p>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>

        <Field label="Notes" error={errors.notes?.message}>
          <Textarea {...form.register('notes')} />
        </Field>
      </form>
    </Dialog>
  );
}

export function CreditNoteList() {
  const { params, setPage, sort, setSort } = useListParams({}, { defaultSort: '-issue_date' });
  const query = creditNotes.useList(params);

  const columns: Column<CreditNote>[] = [
    {
      key: 'credit_note_number',
      header: 'Credit note',
      render: (row) => <span className="font-medium text-slate-900">{row.credit_note_number}</span>,
    },
    {
      key: 'issue_date',
      header: 'Issued',
      sortable: true,
      render: (row) => formatDate(row.issue_date),
    },
    { key: 'reason', header: 'Reason', render: (row) => row.reason ?? '—' },
    {
      key: 'total',
      header: 'Credited',
      sortable: true,
      render: (row) => (
        <span className="tabular-nums">{formatMoney(row.total, row.currency)}</span>
      ),
    },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Credit notes"
        description="Amounts credited back to customers. Each one reduces what its invoice is owed."
      />
      <DataTable
        columns={columns}
        rows={query.data?.data ?? []}
        rowKey={(row) => row.id}
        isLoading={query.isLoading}
        error={query.error}
        pagination={query.data?.pagination}
        onPageChange={setPage}
        sort={sort}
        onSortChange={setSort}
        emptyTitle="No credit notes"
        emptyMessage="Credit notes are raised from an invoice once it has been issued."
      />
    </div>
  );
}
