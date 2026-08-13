import { useNavigate } from 'react-router-dom';
import { Plus } from 'lucide-react';
import { PageHeader } from '@/components/common/PageHeader';
import { DataTable, type Column } from '@/components/common/DataTable';
import { FilterBar } from '@/components/common/Filters';
import { Button } from '@/components/ui/Button';
import { StatusBadge } from '@/components/ui/Badge';
import { useListParams } from '@/hooks/useListParams';
import { quotes } from '@/hooks/useSales';
import { formatDate, formatMoney } from '@/lib/utils';
import { QUOTE_STATUSES, type Quote } from '@/types';

export function QuoteList() {
  const navigate = useNavigate();
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams({
    search: '',
    status: '',
  });
  const query = quotes.useList(params);

  const columns: Column<Quote>[] = [
    {
      key: 'quote_number',
      header: 'Quote',
      sortable: true,
      render: (row) => <span className="font-medium text-slate-900">{row.quote_number}</span>,
    },
    { key: 'status', header: 'Status', render: (row) => <StatusBadge status={row.status} /> },
    { key: 'issue_date', header: 'Issued', sortable: true, render: (row) => formatDate(row.issue_date) },
    { key: 'expiry_date', header: 'Expires', sortable: true, render: (row) => formatDate(row.expiry_date) },
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
        title="Quotes"
        description="Priced proposals awaiting a customer decision"
        actions={
          <Button onClick={() => navigate('/sales/quotes/new')}>
            <Plus className="mr-1 h-4 w-4" />
            New quote
          </Button>
        }
      />

      <FilterBar
        search={{
          value: filters.search,
          onChange: (value) => setFilter('search', value),
          placeholder: 'Search by quote number…',
        }}
        selects={[
          {
            label: 'Status',
            value: filters.status,
            onChange: (value) => setFilter('status', value),
            options: QUOTE_STATUSES,
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
        onRowClick={(row) => navigate(`/sales/quotes/${row.id}`)}
        pagination={query.data?.pagination}
        onPageChange={setPage}
        sort={sort}
        onSortChange={setSort}
        emptyTitle="No quotes yet"
        emptyMessage="Create a quote to start the sales cycle."
        emptyAction={<Button onClick={() => navigate('/sales/quotes/new')}>New quote</Button>}
      />
    </div>
  );
}
