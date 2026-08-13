import { useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { PackageCheck, Plus, Undo2 } from 'lucide-react';
import { PageHeader } from '@/components/common/PageHeader';
import { DataTable, type Column } from '@/components/common/DataTable';
import { FilterBar } from '@/components/common/Filters';
import { SummaryGrid } from '@/components/common/DocumentView';
import { Field, FormGrid } from '@/components/common/Field';
import { LineItemsEditor } from '@/components/common/LineItemsEditor';
import { asLineForm } from '@/lib/lineMath';
import { DetailSkeleton } from '@/components/ui/Skeleton';
import { ErrorState } from '@/components/common/States';
import { Button } from '@/components/ui/Button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card';
import { VendorPaymentsPanel } from './VendorPaymentsPanel';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { Textarea } from '@/components/ui/Textarea';
import { Dialog } from '@/components/ui/Dialog';
import { Badge, StatusBadge } from '@/components/ui/Badge';
import { CurrencyField } from '@/components/common/CurrencyField';
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
  goodsReceipts,
  purchaseOrders,
  purchaseReturns,
  useCreateReceipt,
  useCreateReturn,
  usePurchaseOrderStatus,
  useVendorOptions,
} from '@/hooks/usePurchasing';
import { useProductOptions, useWarehouseOptions } from '@/hooks/useInventory';
import { formatDate, formatMoney, toDateInput, getBaseCurrency } from '@/lib/utils';
import {
  goodsReceiptSchema,
  purchaseOrderSchema,
  purchaseReturnSchema,
  type PurchaseOrderForm,
} from '@/schemas';
import {
  PO_STATUSES,
  type GoodsReceipt,
  type PurchaseOrder,
  type PurchaseReturn,
} from '@/types';

export function PurchaseOrderList() {
  const navigate = useNavigate();
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams({
    search: '',
    status: '',
  });
  const query = purchaseOrders.useList(params);

  const columns: Column<PurchaseOrder>[] = [
    {
      key: 'po_number',
      header: 'PO',
      render: (row) => <span className="font-medium text-slate-900">{row.po_number}</span>,
    },
    { key: 'status', header: 'Status', render: (row) => <StatusBadge status={row.status} /> },
    { key: 'order_date', header: 'Ordered', sortable: true, render: (row) => formatDate(row.order_date) },
    {
      key: 'expected_date',
      header: 'Expected',
      sortable: true,
      render: (row) => formatDate(row.expected_date),
    },
    {
      key: 'total',
      header: 'Total',
      sortable: true,
      align: 'right',
      render: (row) => (
        <span className="font-medium tabular-nums">{formatMoney(row.total, row.currency)}</span>
      ),
    },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Purchase orders"
        description="What you have ordered from suppliers, and what has arrived"
        actions={
          <Button onClick={() => navigate('/purchasing/purchase-orders/new')}>
            <Plus className="mr-1 h-4 w-4" />
            New purchase order
          </Button>
        }
      />

      <FilterBar
        search={{
          value: filters.search,
          onChange: (value) => setFilter('search', value),
          placeholder: 'Search by PO number…',
        }}
        selects={[
          {
            label: 'Status',
            value: filters.status,
            onChange: (value) => setFilter('status', value),
            options: PO_STATUSES,
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
        onRowClick={(row) => navigate(`/purchasing/purchase-orders/${row.id}`)}
        pagination={query.data?.pagination}
        onPageChange={setPage}
        sort={sort}
        onSortChange={setSort}
        emptyTitle="No purchase orders"
        emptyMessage="Raise a PO to order stock from a supplier."
        emptyAction={
          <Button onClick={() => navigate('/purchasing/purchase-orders/new')}>
            New purchase order
          </Button>
        }
      />
    </div>
  );
}

export function PurchaseOrderForm() {
  const navigate = useNavigate();
  const { options: vendorOptions } = useVendorOptions();
  const { options: productOptions } = useProductOptions();

  const create = purchaseOrders.useCreate({
    successMessage: 'Purchase order created',
    onSuccess: (po) => navigate(`/purchasing/purchase-orders/${po.id}`),
  });

  const form = useForm<PurchaseOrderForm>({
    resolver: zodResolver(purchaseOrderSchema),
    defaultValues: {
      vendor_id: '',
      order_date: toDateInput(),
      expected_date: '',
      notes: '',
      lines: [{ description: '', quantity: 1, unit_price: 0, tax_rate: 0 }],
    },
  });

  const { errors } = form.formState;
  // One currency per installation, so this is not a per-document choice.
  const currency = getBaseCurrency();

  const onSubmit = form.handleSubmit((values) =>
    create.mutate({
      ...values,
      lines: values.lines.map((line) => ({
        ...line,
        product_id: line.product_id || undefined,
        quantity: Number(line.quantity),
        unit_price: Number(line.unit_price).toFixed(2),
        tax_rate: Number(line.tax_rate ?? 0).toFixed(2),
      })),
    })
  );

  return (
    <form onSubmit={onSubmit} className="space-y-6" noValidate>
      <PageHeader
        title="New purchase order"
        backTo="/purchasing/purchase-orders"
        backLabel="Back to purchase orders"
        actions={
          <>
            <Button
              type="button"
              variant="outline"
              onClick={() => navigate('/purchasing/purchase-orders')}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={create.isPending}>
              {create.isPending ? 'Saving…' : 'Create purchase order'}
            </Button>
          </>
        }
      />

      <Card>
        <CardContent className="pt-6">
          <FormGrid>
            <Field label="Vendor" required error={errors.vendor_id?.message}>
              <Select
                options={vendorOptions}
                placeholder="Select a vendor"
                {...form.register('vendor_id')}
              />
            </Field>
            <CurrencyField {...form.register('currency')} />
            <Field label="Order date" required error={errors.order_date?.message}>
              <Input type="date" {...form.register('order_date')} />
            </Field>
            <Field label="Expected date" error={errors.expected_date?.message}>
              <Input type="date" {...form.register('expected_date')} />
            </Field>
          </FormGrid>
        </CardContent>
      </Card>

      <Card>
        <CardContent className="pt-6">
          {/* Purchase order lines carry no discount column. */}
          <LineItemsEditor
            form={asLineForm(form)}
            productOptions={productOptions}
            showDiscount={false}
            currency={currency}
            disabled={create.isPending}
          />
        </CardContent>
      </Card>

      <Card>
        <CardContent className="pt-6">
          <Field label="Notes" error={errors.notes?.message}>
            <Textarea {...form.register('notes')} />
          </Field>
        </CardContent>
      </Card>
    </form>
  );
}

/** Only the transitions a user drives; receiving statuses are set by receipts. */
const NEXT_STATUS: Record<string, Array<{ status: string; label: string }>> = {
  draft: [
    { status: 'sent', label: 'Send to vendor' },
    { status: 'cancelled', label: 'Cancel' },
  ],
  sent: [
    { status: 'confirmed', label: 'Mark confirmed' },
    { status: 'cancelled', label: 'Cancel' },
  ],
  confirmed: [{ status: 'cancelled', label: 'Cancel' }],
  partially_received: [{ status: 'closed', label: 'Close order' }],
  fully_received: [{ status: 'closed', label: 'Close order' }],
};

export function PurchaseOrderDetail() {
  const { id } = useParams<{ id: string }>();
  const query = purchaseOrders.useOne(id);
  const setStatus = usePurchaseOrderStatus();
  const [receiving, setReceiving] = useState(false);
  const [returning, setReturning] = useState(false);

  if (query.isLoading) return <DetailSkeleton />;
  if (query.error) return <ErrorState error={query.error} onRetry={query.refetch} />;
  if (!query.data) return null;

  const order = query.data;
  const transitions = NEXT_STATUS[order.status] ?? [];
  const canReceive = ['confirmed', 'partially_received'].includes(order.status);
  // Wider than receiving: a fully received order is the one most likely to have
  // something wrong with it.
  const received = order.lines.reduce((sum, line) => sum + line.received_quantity, 0);
  const canReturn =
    ['confirmed', 'partially_received', 'fully_received'].includes(order.status) && received > 0;
  const outstanding = order.lines.reduce((sum, line) => sum + line.outstanding, 0);

  return (
    <div className="space-y-6">
      <PageHeader
        title={order.po_number}
        backTo="/purchasing/purchase-orders"
        backLabel="Back to purchase orders"
        badge={<StatusBadge status={order.status} />}
        actions={
          <>
            {transitions.map((transition) => (
              <Button
                key={transition.status}
                variant="outline"
                disabled={setStatus.isPending}
                onClick={() => setStatus.mutate({ id: order.id, status: transition.status })}
              >
                {transition.label}
              </Button>
            ))}
            {canReturn && (
              <Button variant="outline" onClick={() => setReturning(true)}>
                <Undo2 className="mr-1 h-4 w-4" />
                Send back
              </Button>
            )}
            {canReceive && (
              <Button onClick={() => setReceiving(true)}>
                <PackageCheck className="mr-1 h-4 w-4" />
                Receive goods
              </Button>
            )}
          </>
        }
      />

      <SummaryGrid
        items={[
          { label: 'Ordered', value: formatDate(order.order_date) },
          { label: 'Expected', value: formatDate(order.expected_date) },
          { label: 'Total', value: formatMoney(order.total, order.currency) },
          { label: 'Still outstanding', value: `${outstanding} unit(s)` },
        ]}
      />

      <VendorPaymentsPanel order={order} />

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Line items</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow className="hover:bg-transparent">
                <TableHead>Description</TableHead>
                <TableHead className="text-right">Ordered</TableHead>
                <TableHead className="text-right">Received</TableHead>
                <TableHead className="text-right">Outstanding</TableHead>
                <TableHead className="text-right">Unit price</TableHead>
                <TableHead className="text-right">Line total</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {order.lines.map((line) => (
                <TableRow key={line.id}>
                  <TableCell className="font-medium">{line.description}</TableCell>
                  <TableCell className="text-right tabular-nums">{line.quantity}</TableCell>
                  <TableCell className="text-right tabular-nums">{line.received_quantity}</TableCell>
                  <TableCell className="text-right tabular-nums">
                    {line.is_fully_received ? (
                      <Badge tone="success">Complete</Badge>
                    ) : (
                      line.outstanding
                    )}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {formatMoney(line.unit_price, order.currency)}
                  </TableCell>
                  <TableCell className="text-right font-medium tabular-nums">
                    {formatMoney(line.line_total, order.currency)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>

          <div className="mt-4 flex justify-end">
            <dl className="w-full max-w-xs space-y-1 text-sm">
              <div className="flex justify-between">
                <dt className="text-slate-500">Subtotal</dt>
                <dd className="tabular-nums">{formatMoney(order.subtotal, order.currency)}</dd>
              </div>
              <div className="flex justify-between">
                <dt className="text-slate-500">Tax</dt>
                <dd className="tabular-nums">{formatMoney(order.tax_amount, order.currency)}</dd>
              </div>
              <div className="flex justify-between border-t pt-1 text-base font-semibold">
                <dt>Total</dt>
                <dd className="tabular-nums">{formatMoney(order.total, order.currency)}</dd>
              </div>
            </dl>
          </div>
        </CardContent>
      </Card>

      {receiving && (
        <ReceiveGoodsDialog
          poId={order.id}
          lines={order.lines.map((line) => ({
            po_line_id: line.id,
            description: line.description,
            outstanding: line.outstanding,
          }))}
          onClose={() => setReceiving(false)}
        />
      )}
      {returning && (
        <SendBackDialog
          poId={order.id}
          lines={order.lines
            .filter((line) => line.received_quantity > 0)
            .map((line) => ({
              po_line_id: line.id,
              description: line.description,
              received: line.received_quantity,
            }))}
          onClose={() => setReturning(false)}
        />
      )}
    </div>
  );
}

function ReceiveGoodsDialog({
  poId,
  lines,
  onClose,
}: {
  poId: string;
  lines: Array<{ po_line_id: string; description: string; outstanding: number }>;
  onClose: () => void;
}) {
  const receive = useCreateReceipt();
  const { options: warehouseOptions } = useWarehouseOptions();

  const form = useForm({
    resolver: zodResolver(goodsReceiptSchema),
    defaultValues: {
      warehouse_id: '',
      receipt_date: toDateInput(),
      notes: '',
      // Pre-filled with everything still outstanding — the common case.
      lines: lines.map((line) => ({ ...line, quantity_received: line.outstanding })),
    },
  });

  const { errors } = form.formState;

  const onSubmit = form.handleSubmit((values) =>
    receive.mutate(
      {
        po_id: poId,
        warehouse_id: values.warehouse_id,
        receipt_date: values.receipt_date || undefined,
        notes: values.notes || undefined,
        // Zero-quantity lines mean "not in this delivery".
        lines: values.lines
          .filter((line) => Number(line.quantity_received) > 0)
          .map((line) => ({
            po_line_id: line.po_line_id,
            quantity_received: Number(line.quantity_received),
          })),
      },
      { onSuccess: onClose }
    )
  );

  return (
    <Dialog
      open
      onClose={onClose}
      title="Receive goods"
      description="Received quantities are added to stock in the chosen warehouse."
      className="max-w-2xl"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={receive.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={receive.isPending}>
            {receive.isPending ? 'Receiving…' : 'Receive into stock'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <FormGrid>
          <Field label="Warehouse" required error={errors.warehouse_id?.message}>
            <Select
              options={warehouseOptions}
              placeholder="Where the goods arrive"
              {...form.register('warehouse_id')}
            />
          </Field>
          <Field label="Receipt date" error={errors.receipt_date?.message}>
            <Input type="date" {...form.register('receipt_date')} />
          </Field>
        </FormGrid>

        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-slate-900">Quantities received</h3>
          <div className="overflow-hidden rounded-md border">
            <table className="w-full text-sm">
              <thead className="border-b bg-slate-50">
                <tr>
                  <th className="px-3 py-2 text-left text-xs font-medium uppercase text-slate-500">
                    Item
                  </th>
                  <th className="w-28 px-3 py-2 text-right text-xs font-medium uppercase text-slate-500">
                    Outstanding
                  </th>
                  <th className="w-32 px-3 py-2 text-right text-xs font-medium uppercase text-slate-500">
                    Receiving
                  </th>
                </tr>
              </thead>
              <tbody>
                {lines.map((line, index) => {
                  const rowError = (errors.lines ?? [])[index] as
                    | Record<string, { message?: string }>
                    | undefined;

                  return (
                    <tr key={line.po_line_id} className="border-b last:border-0">
                      <td className="px-3 py-2">{line.description}</td>
                      <td className="px-3 py-2 text-right tabular-nums text-slate-500">
                        {line.outstanding}
                      </td>
                      <td className="px-3 py-2">
                        <Input
                          type="number"
                          min={0}
                          max={line.outstanding}
                          className="text-right"
                          {...form.register(`lines.${index}.quantity_received`)}
                        />
                        {rowError?.quantity_received?.message && (
                          <p className="mt-1 text-xs text-red-600">
                            {rowError.quantity_received.message}
                          </p>
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

export function GoodsReceiptList() {
  const navigate = useNavigate();
  const { params, setPage, sort, setSort } = useListParams({}, { defaultSort: '-receipt_date' });
  const query = goodsReceipts.useList(params);

  const columns: Column<GoodsReceipt>[] = [
    {
      key: 'receipt_number',
      header: 'Receipt',
      render: (row) => <span className="font-medium text-slate-900">{row.receipt_number}</span>,
    },
    {
      key: 'receipt_date',
      header: 'Received',
      sortable: true,
      render: (row) => formatDate(row.receipt_date),
    },
    { key: 'status', header: 'Status', render: (row) => <StatusBadge status={row.status} /> },
    { key: 'notes', header: 'Notes', render: (row) => row.notes ?? '—' },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Goods receipts"
        description="Every delivery booked in against a purchase order"
      />

      <DataTable
        columns={columns}
        rows={query.data?.data}
        rowKey={(row) => row.id}
        isLoading={query.isLoading}
        error={query.error}
        onRetry={query.refetch}
        onRowClick={(row) => navigate(`/purchasing/purchase-orders/${row.po_id}`)}
        pagination={query.data?.pagination}
        onPageChange={setPage}
        sort={sort}
        onSortChange={setSort}
        emptyTitle="No goods received yet"
        emptyMessage="Confirm a purchase order, then receive the delivery against it."
      />
    </div>
  );
}

/**
 * Sending goods back to the supplier.
 *
 * The mirror of [`ReceiveGoodsDialog`], with one deliberate difference: the
 * quantities start at **zero** rather than pre-filled. Receiving everything
 * outstanding is the common case; returning everything is not, and a form that
 * defaults to sending the whole delivery back is one stray click from a large
 * credit note.
 */
function SendBackDialog({
  poId,
  lines,
  onClose,
}: {
  poId: string;
  lines: Array<{ po_line_id: string; description: string; received: number }>;
  onClose: () => void;
}) {
  const sendBack = useCreateReturn();
  const { options: warehouseOptions } = useWarehouseOptions();

  const form = useForm({
    resolver: zodResolver(purchaseReturnSchema),
    defaultValues: {
      warehouse_id: '',
      return_date: toDateInput(),
      reason: '',
      notes: '',
      lines: lines.map((line) => ({ ...line, quantity_returned: 0 })),
    },
  });

  const { errors } = form.formState;

  const onSubmit = form.handleSubmit((values) =>
    sendBack.mutate(
      {
        po_id: poId,
        warehouse_id: values.warehouse_id,
        return_date: values.return_date || undefined,
        reason: values.reason || undefined,
        notes: values.notes || undefined,
        lines: values.lines
          .filter((line) => Number(line.quantity_returned) > 0)
          .map((line) => ({
            po_line_id: line.po_line_id,
            quantity_returned: Number(line.quantity_returned),
          })),
      },
      { onSuccess: onClose }
    )
  );

  return (
    <Dialog
      open
      onClose={onClose}
      title="Send goods back"
      description="The stock leaves the warehouse and the supplier is credited, reducing what this order owes."
      className="max-w-2xl"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={sendBack.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={sendBack.isPending}>
            {sendBack.isPending ? 'Sending…' : 'Send back'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <FormGrid>
          <Field label="Warehouse" required error={errors.warehouse_id?.message}>
            {/* `Field` renders the label but does not associate it with the
                control, so without this the select has no accessible name. */}
            <Select
              options={warehouseOptions}
              placeholder="Where the goods leave from"
              aria-label="Warehouse"
              {...form.register('warehouse_id')}
            />
          </Field>
          <Field label="Return date" error={errors.return_date?.message}>
            <Input type="date" {...form.register('return_date')} />
          </Field>
        </FormGrid>

        <Field label="Reason" error={errors.reason?.message}>
          <Input placeholder="Arrived damaged, wrong item, over-delivered…" {...form.register('reason')} />
        </Field>

        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-slate-900">Quantities going back</h3>
          <div className="overflow-hidden rounded-md border">
            <table className="w-full text-sm">
              <thead className="border-b bg-slate-50">
                <tr>
                  <th className="px-3 py-2 text-left text-xs font-medium uppercase text-slate-500">
                    Item
                  </th>
                  <th className="w-28 px-3 py-2 text-right text-xs font-medium uppercase text-slate-500">
                    On hand
                  </th>
                  <th className="w-32 px-3 py-2 text-right text-xs font-medium uppercase text-slate-500">
                    Returning
                  </th>
                </tr>
              </thead>
              <tbody>
                {lines.map((line, index) => {
                  const rowError = (errors.lines ?? [])[index] as
                    | Record<string, { message?: string }>
                    | undefined;

                  return (
                    <tr key={line.po_line_id} className="border-b last:border-0">
                      <td className="px-3 py-2">{line.description}</td>
                      <td className="px-3 py-2 text-right tabular-nums text-slate-500">
                        {line.received}
                      </td>
                      <td className="px-3 py-2">
                        <Input
                          type="number"
                          min={0}
                          max={line.received}
                          className="text-right"
                          {...form.register(`lines.${index}.quantity_returned`)}
                        />
                        {rowError?.quantity_returned?.message && (
                          <p className="mt-1 text-xs text-red-600">
                            {rowError.quantity_returned.message}
                          </p>
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

export function PurchaseReturnList() {
  const { params, setPage, sort, setSort } = useListParams({}, { defaultSort: '-return_date' });
  const query = purchaseReturns.useList(params);

  const columns: Column<PurchaseReturn>[] = [
    {
      key: 'return_number',
      header: 'Return',
      render: (row) => <span className="font-medium text-slate-900">{row.return_number}</span>,
    },
    {
      key: 'return_date',
      header: 'Sent back',
      sortable: true,
      render: (row) => formatDate(row.return_date),
    },
    { key: 'reason', header: 'Reason', render: (row) => row.reason ?? '—' },
    { key: 'notes', header: 'Notes', render: (row) => row.notes ?? '—' },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Purchase returns"
        description="Goods sent back to suppliers. Each one credits the order and takes the stock off the shelf."
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
        emptyTitle="Nothing has been sent back"
        emptyMessage="Returns are raised from a purchase order once goods have arrived."
      />
    </div>
  );
}
