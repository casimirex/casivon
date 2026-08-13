import { useNavigate, useParams } from 'react-router-dom';
import { ArrowRight } from 'lucide-react';
import { PageHeader } from '@/components/common/PageHeader';
import { DataTable, type Column } from '@/components/common/DataTable';
import { FilterBar } from '@/components/common/Filters';
import { DocumentLines, SummaryGrid } from '@/components/common/DocumentView';
import { DetailSkeleton } from '@/components/ui/Skeleton';
import { ErrorState } from '@/components/common/States';
import { Button } from '@/components/ui/Button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card';
import { StatusBadge } from '@/components/ui/Badge';
import { useListParams } from '@/hooks/useListParams';
import { orders, useConvertOrderToInvoice, useOrderStatus } from '@/hooks/useSales';
import { useOrganization } from '@/hooks/useSettings';
import { formatDate, formatMoney } from '@/lib/utils';
import { ORDER_STATUSES, type SalesOrder } from '@/types';

export function OrderList() {
  const navigate = useNavigate();
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams({
    search: '',
    status: '',
  });
  const query = orders.useList(params);

  const columns: Column<SalesOrder>[] = [
    {
      key: 'order_number',
      header: 'Order',
      render: (row) => <span className="font-medium text-slate-900">{row.order_number}</span>,
    },
    { key: 'status', header: 'Status', render: (row) => <StatusBadge status={row.status} /> },
    { key: 'order_date', header: 'Ordered', sortable: true, render: (row) => formatDate(row.order_date) },
    {
      key: 'required_date',
      header: 'Required',
      sortable: true,
      render: (row) => formatDate(row.required_date),
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
      <PageHeader title="Sales orders" description="Confirmed customer orders moving to fulfilment" />

      <FilterBar
        search={{
          value: filters.search,
          onChange: (value) => setFilter('search', value),
          placeholder: 'Search by order number…',
        }}
        selects={[
          {
            label: 'Status',
            value: filters.status,
            onChange: (value) => setFilter('status', value),
            options: ORDER_STATUSES,
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
        onRowClick={(row) => navigate(`/sales/orders/${row.id}`)}
        pagination={query.data?.pagination}
        onPageChange={setPage}
        sort={sort}
        onSortChange={setSort}
        emptyTitle="No sales orders"
        emptyMessage="Accept a quote and convert it to create the first order."
        emptyAction={<Button onClick={() => navigate('/sales/quotes')}>Go to quotes</Button>}
      />
    </div>
  );
}

/** Only the transitions `OrderStatus::can_transition` permits. */
const NEXT_STATUS: Record<string, Array<{ status: string; label: string }>> = {
  draft: [
    { status: 'confirmed', label: 'Confirm order' },
    { status: 'cancelled', label: 'Cancel' },
  ],
  confirmed: [
    { status: 'processing', label: 'Start processing' },
    { status: 'cancelled', label: 'Cancel' },
  ],
  processing: [
    { status: 'shipped', label: 'Mark shipped' },
    { status: 'cancelled', label: 'Cancel' },
  ],
  // Reached by part-invoicing the order, never asked for directly.
  partially_shipped: [
    { status: 'shipped', label: 'Mark shipped' },
    { status: 'cancelled', label: 'Cancel' },
  ],
  shipped: [{ status: 'delivered', label: 'Mark delivered' }],
};

export function OrderDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const query = orders.useOne(id);
  const setStatus = useOrderStatus();
  const organization = useOrganization();
  const convert = useConvertOrderToInvoice();

  if (query.isLoading) return <DetailSkeleton />;
  if (query.error) return <ErrorState error={query.error} onRetry={query.refetch} />;
  if (!query.data) return null;

  const order = query.data;
  const transitions = NEXT_STATUS[order.status] ?? [];
  // Anything from `confirmed` onward can be billed, and only while something is
  // still outstanding — an order billed in full has nothing left to raise.
  const outstanding = order.lines.reduce((total, line) => total + (line.outstanding ?? 0), 0);
  const canInvoice =
    outstanding > 0 &&
    ['confirmed', 'processing', 'partially_shipped', 'shipped', 'delivered'].includes(order.status);
  const busy = setStatus.isPending || convert.isPending;

  // Where invoicing is what takes goods off the shelf, an order cannot claim
  // they have gone until it has been invoiced *in full*. Said up front rather
  // than left for the server to refuse: the button would otherwise look
  // available and then fail, which is a worse way to learn the rule.
  const shipsOnInvoice = Boolean(organization.data?.default_dispatch_warehouse_id);
  const shippingNeedsAnInvoice =
    shipsOnInvoice && outstanding > 0 && transitions.some((t) => t.status === 'shipped');

  return (
    <div className="space-y-6">
      <PageHeader
        title={order.order_number}
        backTo="/sales/orders"
        backLabel="Back to orders"
        badge={<StatusBadge status={order.status} />}
        actions={
          <>
            {transitions.map((transition) => (
              <Button
                key={transition.status}
                variant="outline"
                disabled={busy}
                onClick={() => setStatus.mutate({ id: order.id, status: transition.status })}
              >
                {transition.label}
              </Button>
            ))}
            {canInvoice && (
              <Button
                disabled={busy}
                onClick={() =>
                  convert.mutate(
                    { id: order.id, payment_terms_days: 30 },
                    { onSuccess: (invoice) => navigate(`/sales/invoices/${invoice.id}`) }
                  )
                }
              >
                Create invoice
                <ArrowRight className="ml-1 h-4 w-4" />
              </Button>
            )}
          </>
        }
      />

      {shippingNeedsAnInvoice && (
        <p className="text-sm text-slate-500">
          Issuing this order&rsquo;s invoice is what takes the goods off the shelf, so it can only
          be marked shipped once every line has been invoiced. {outstanding} unit
          {outstanding === 1 ? '' : 's'} still to bill.
        </p>
      )}

      <SummaryGrid
        items={[
          { label: 'Ordered', value: formatDate(order.order_date) },
          { label: 'Required', value: formatDate(order.required_date) },
          { label: 'Total', value: formatMoney(order.total, order.currency) },
          {
            label: 'From quote',
            value: order.quote_id ? (
              <button
                onClick={() => navigate(`/sales/quotes/${order.quote_id}`)}
                className="text-base text-primary hover:underline"
              >
                View quote
              </button>
            ) : (
              '—'
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
            lines={order.lines}
            currency={order.currency}
            subtotal={order.subtotal}
            tax={order.tax_amount}
            total={order.total}
            baseTotal={order.base_total}
            fxRate={order.fx_rate}
          />
        </CardContent>
      </Card>

      {order.lines.some((line) => (line.invoiced_quantity ?? 0) > 0) && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Still to bill</CardTitle>
          </CardHeader>
          <CardContent>
            {/* Only once something has been billed: on an untouched order every
                line's outstanding is its whole quantity, which the line table
                above already says. */}
            <dl className="space-y-2 text-sm">
              {order.lines.map((line) => (
                <div key={line.id} className="flex justify-between gap-4">
                  <dt className="text-slate-600">{line.description}</dt>
                  <dd className="tabular-nums text-slate-900">
                    {line.outstanding ?? 0} of {line.quantity} outstanding
                  </dd>
                </div>
              ))}
            </dl>
          </CardContent>
        </Card>
      )}

      {(order.shipping_address || order.billing_address) && (
        <div className="grid gap-4 sm:grid-cols-2">
          {order.shipping_address && (
            <Card>
              <CardHeader>
                <CardTitle className="text-base">Shipping address</CardTitle>
              </CardHeader>
              <CardContent>
                <p className="whitespace-pre-wrap text-sm text-slate-600">{order.shipping_address}</p>
              </CardContent>
            </Card>
          )}
          {order.billing_address && (
            <Card>
              <CardHeader>
                <CardTitle className="text-base">Billing address</CardTitle>
              </CardHeader>
              <CardContent>
                <p className="whitespace-pre-wrap text-sm text-slate-600">{order.billing_address}</p>
              </CardContent>
            </Card>
          )}
        </div>
      )}
    </div>
  );
}
