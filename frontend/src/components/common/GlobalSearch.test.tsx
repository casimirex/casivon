import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useLocation } from 'react-router-dom';
import { renderWithProviders } from '@/test/renderWithProviders';
import { GlobalSearch } from './GlobalSearch';
import { http } from '@/api/client';
import type { SearchHit } from '@/types';

/// `MemoryRouter` keeps its location to itself rather than on `window`, so
/// navigation is asserted through the router that actually performed it.
function WithLocation() {
  const location = useLocation();
  return (
    <>
      <GlobalSearch />
      <span data-testid="path">{location.pathname}</span>
    </>
  );
}

const HITS: SearchHit[] = [
  { kind: 'company', id: 'co-1', title: 'Northwind Trading', subtitle: 'ap@northwind.test' },
  { kind: 'invoice', id: 'inv-1', title: 'INV-2026-000014', subtitle: 'sent' },
  { kind: 'invoice', id: 'inv-2', title: 'INV-2026-000021', subtitle: 'paid' },
];

function mockSearch(hits: SearchHit[] = HITS) {
  return vi
    .spyOn(http, 'get')
    .mockResolvedValue({ query: 'north', hits } as never);
}

describe('<GlobalSearch />', () => {
  beforeEach(() => vi.restoreAllMocks());

  it('groups what it finds by kind', async () => {
    mockSearch();
    renderWithProviders(<GlobalSearch />);

    await userEvent.type(screen.getByRole('combobox', { name: 'Search' }), 'north');

    expect(await screen.findByText('Northwind Trading')).toBeInTheDocument();
    // Headings, so a list of mixed results is readable at a glance.
    expect(screen.getByText('Companies')).toBeInTheDocument();
    expect(screen.getByText('Invoices')).toBeInTheDocument();
    expect(screen.getAllByRole('option')).toHaveLength(3);
  });

  it('waits for typing to stop before asking', async () => {
    const get = mockSearch();
    renderWithProviders(<GlobalSearch />);

    await userEvent.type(screen.getByRole('combobox', { name: 'Search' }), 'north');

    // Five keystrokes, one request — four of whose answers would have been
    // thrown away, and not necessarily in order.
    await waitFor(() => expect(get).toHaveBeenCalledTimes(1));
    expect(get).toHaveBeenCalledWith('/search?q=north');
  });

  it('does not ask for a term too short to mean anything', async () => {
    const get = mockSearch();
    renderWithProviders(<GlobalSearch />);

    await userEvent.type(screen.getByRole('combobox', { name: 'Search' }), 'n');

    expect(await screen.findByText(/Keep typing/)).toBeInTheDocument();
    expect(get).not.toHaveBeenCalled();
  });

  it('navigates to the record the user picks', async () => {
    mockSearch();
    renderWithProviders(<WithLocation />);

    await userEvent.type(screen.getByRole('combobox', { name: 'Search' }), 'north');
    await userEvent.click(await screen.findByText('INV-2026-000014'));

    // The route table lives here, not in the API response — the server sends
    // `kind` and `id` and nothing about URLs.
    expect(screen.getByTestId('path')).toHaveTextContent('/sales/invoices/inv-1');
  });

  it('moves through results with the arrow keys', async () => {
    mockSearch();
    renderWithProviders(<WithLocation />);

    const box = screen.getByRole('combobox', { name: 'Search' });
    await userEvent.type(box, 'north');
    await screen.findByText('Northwind Trading');

    // First result is selected to begin with, so Enter alone picks the obvious
    // one without touching the arrows.
    expect(screen.getAllByRole('option')[0]).toHaveAttribute('aria-selected', 'true');

    await userEvent.keyboard('{ArrowDown}');
    expect(screen.getAllByRole('option')[1]).toHaveAttribute('aria-selected', 'true');

    await userEvent.keyboard('{Enter}');
    expect(screen.getByTestId('path')).toHaveTextContent('/sales/invoices/inv-1');
  });

  it('says so when nothing matches', async () => {
    mockSearch([]);
    renderWithProviders(<GlobalSearch />);

    await userEvent.type(screen.getByRole('combobox', { name: 'Search' }), 'north');

    expect(await screen.findByText(/Nothing matches/)).toBeInTheDocument();
  });

  it('ignores a kind it has no destination for', async () => {
    // The backend could gain a searchable kind before this table does; an
    // unroutable result is dropped rather than rendered as a dead link.
    mockSearch([{ kind: 'something_new', id: 'x-1', title: 'Mystery', subtitle: null }]);
    renderWithProviders(<GlobalSearch />);

    await userEvent.type(screen.getByRole('combobox', { name: 'Search' }), 'north');

    expect(await screen.findByText(/Nothing matches/)).toBeInTheDocument();
    expect(screen.queryByText('Mystery')).not.toBeInTheDocument();
  });
});
