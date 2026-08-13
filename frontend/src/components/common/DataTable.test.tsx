import { describe, expect, it, vi } from 'vitest';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '@/test/renderWithProviders';
import { DataTable, type Column } from './DataTable';
import { ApiError } from '@/api/client';

interface Row {
  id: string;
  name: string;
  total: string;
}

const columns: Column<Row>[] = [
  { key: 'name', header: 'Name', sortable: true, render: (row) => row.name },
  { key: 'total', header: 'Total', sortable: true, align: 'right', render: (row) => row.total },
];

const rows: Row[] = [
  { id: '1', name: 'Widget', total: '10.00' },
  { id: '2', name: 'Gadget', total: '20.00' },
];

/**
 * Every list screen in the app is this component, so its four states and its
 * sort/paginate contract are worth pinning down once here.
 */
describe('<DataTable />', () => {
  it('renders rows', () => {
    renderWithProviders(<DataTable columns={columns} rows={rows} rowKey={(row) => row.id} />);
    expect(screen.getByText('Widget')).toBeInTheDocument();
    expect(screen.getByText('Gadget')).toBeInTheDocument();
  });

  it('shows a skeleton while loading rather than an empty table', () => {
    renderWithProviders(
      <DataTable columns={columns} rows={undefined} rowKey={(row) => row.id} isLoading />
    );
    expect(screen.getByLabelText('Loading')).toBeInTheDocument();
    expect(screen.queryByText('Widget')).not.toBeInTheDocument();
  });

  it('shows the empty state with its call to action', () => {
    renderWithProviders(
      <DataTable
        columns={columns}
        rows={[]}
        rowKey={(row) => row.id}
        emptyTitle="No products yet"
        emptyMessage="Add the goods you sell."
      />
    );
    expect(screen.getByText('No products yet')).toBeInTheDocument();
    expect(screen.getByText('Add the goods you sell.')).toBeInTheDocument();
  });

  it("surfaces the server's message and offers a retry", async () => {
    const onRetry = vi.fn();
    renderWithProviders(
      <DataTable
        columns={columns}
        rows={undefined}
        rowKey={(row) => row.id}
        error={new ApiError('Database unavailable', 500)}
        onRetry={onRetry}
      />
    );

    expect(screen.getByText('Database unavailable')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /try again/i }));
    expect(onRetry).toHaveBeenCalledOnce();
  });

  it('does not offer a retry on a 403, which retrying cannot fix', () => {
    renderWithProviders(
      <DataTable
        columns={columns}
        rows={undefined}
        rowKey={(row) => row.id}
        error={new ApiError('Forbidden', 403)}
        onRetry={vi.fn()}
      />
    );

    expect(screen.getByText(/do not have access/i)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /try again/i })).not.toBeInTheDocument();
  });

  it('asks for ascending sort first, then flips to descending', async () => {
    const onSortChange = vi.fn();
    const { rerender } = renderWithProviders(
      <DataTable
        columns={columns}
        rows={rows}
        rowKey={(row) => row.id}
        sort="-created_at"
        onSortChange={onSortChange}
      />
    );

    await userEvent.click(screen.getByRole('button', { name: /sort by name/i }));
    expect(onSortChange).toHaveBeenLastCalledWith('name');

    // Now that `name` is the active ascending sort, clicking flips it.
    rerender(
      <DataTable
        columns={columns}
        rows={rows}
        rowKey={(row) => row.id}
        sort="name"
        onSortChange={onSortChange}
      />
    );
    await userEvent.click(screen.getByRole('button', { name: /sort by name/i }));
    expect(onSortChange).toHaveBeenLastCalledWith('-name');
  });

  it('reports the visible range and moves between pages', async () => {
    const onPageChange = vi.fn();
    renderWithProviders(
      <DataTable
        columns={columns}
        rows={rows}
        rowKey={(row) => row.id}
        pagination={{ page: 2, per_page: 20, total: 45, total_pages: 3 }}
        onPageChange={onPageChange}
      />
    );

    expect(screen.getByText('21')).toBeInTheDocument();
    expect(screen.getByText('40')).toBeInTheDocument();
    expect(screen.getByText('45')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: /next/i }));
    expect(onPageChange).toHaveBeenCalledWith(3);

    await userEvent.click(screen.getByRole('button', { name: /previous/i }));
    expect(onPageChange).toHaveBeenCalledWith(1);
  });

  it('hides pagination when everything fits on one page', () => {
    renderWithProviders(
      <DataTable
        columns={columns}
        rows={rows}
        rowKey={(row) => row.id}
        pagination={{ page: 1, per_page: 20, total: 2, total_pages: 1 }}
        onPageChange={vi.fn()}
      />
    );
    expect(screen.queryByRole('button', { name: /next/i })).not.toBeInTheDocument();
  });

  it('opens a row when the table is clickable', async () => {
    const onRowClick = vi.fn();
    renderWithProviders(
      <DataTable columns={columns} rows={rows} rowKey={(row) => row.id} onRowClick={onRowClick} />
    );

    await userEvent.click(screen.getByText('Widget'));
    expect(onRowClick).toHaveBeenCalledWith(rows[0]);
  });
});
