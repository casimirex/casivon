import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen } from '@testing-library/react';
import { Route, Routes } from 'react-router-dom';
import { renderWithProviders } from '@/test/renderWithProviders';
import { OrderDetail } from './OrderPages';
import { http } from '@/api/client';
import { setBaseCurrency } from '@/lib/utils';

const WAREHOUSE_ID = '11111111-1111-4111-8111-111111111111';
const ORDER_ID = '22222222-2222-4222-8222-222222222222';

/// `invoiced` decides how much of the order is still to bill, which is what the
/// note is now about: shipping needs *every* line invoiced, not merely one
/// invoice somewhere.
function order(status: string, invoiced = 0) {
  return {
    id: ORDER_ID,
    order_number: 'SO-2026-000001',
    customer_id: 'c-1',
    status,
    order_date: '2026-03-01',
    currency: 'USD',
    total: '200.00',
    lines: [
      {
        id: '33333333-3333-4333-8333-333333333333',
        description: 'Widget',
        quantity: 10,
        unit_price: '20.00',
        discount_percent: '0',
        tax_rate: '0',
        line_total: '200.00',
        sort_order: 0,
        invoiced_quantity: invoiced,
        outstanding: 10 - invoiced,
        is_fully_invoiced: invoiced >= 10,
      },
    ],
  };
}

function AtOrder() {
  return (
    <Routes>
      <Route path="/sales/orders/:id" element={<OrderDetail />} />
    </Routes>
  );
}

/// `dispatching` decides whether the organisation ships automatically, which is
/// the only thing that makes the rule apply.
function mockApi(status: string, dispatching: boolean, invoiced = 0) {
  vi.spyOn(http, 'get').mockImplementation(((path: string) => {
    if (path.includes('organization')) {
      return Promise.resolve({
        name: 'Globex',
        default_currency: 'USD',
        default_dispatch_warehouse_id: dispatching ? WAREHOUSE_ID : null,
      });
    }
    return Promise.resolve(order(status, invoiced));
  }) as never);
}

describe('the order lifecycle note', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    setBaseCurrency('USD');
  });

  it('says why shipping is not available yet', async () => {
    mockApi('processing', true);
    renderWithProviders(<AtOrder />, { route: `/sales/orders/${ORDER_ID}` });

    // Said up front rather than left for the server to refuse — the button is
    // still offered, but the rule is no longer a surprise.
    expect(
      await screen.findByText(/takes the goods off the shelf/)
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Mark shipped' })).toBeInTheDocument();
  });

  it('stays quiet when the organisation does not ship automatically', async () => {
    mockApi('processing', false);
    renderWithProviders(<AtOrder />, { route: `/sales/orders/${ORDER_ID}` });

    // No dispatch warehouse means invoicing moves no stock, so there is no rule
    // to explain and no restriction to apologise for.
    expect(await screen.findByRole('button', { name: 'Mark shipped' })).toBeInTheDocument();
    expect(screen.queryByText(/takes the goods off the shelf/)).not.toBeInTheDocument();
  });

  it('counts what is left to bill', async () => {
    mockApi('partially_shipped', true, 6);
    renderWithProviders(<AtOrder />, { route: `/sales/orders/${ORDER_ID}` });

    expect(await screen.findByText(/4 units still to bill/)).toBeInTheDocument();
  });

  it('stays quiet once every line is invoiced', async () => {
    mockApi('processing', true, 10);
    renderWithProviders(<AtOrder />, { route: `/sales/orders/${ORDER_ID}` });

    // Nothing outstanding, so shipping is available and there is no rule left
    // to explain.
    expect(await screen.findByRole('button', { name: 'Mark shipped' })).toBeInTheDocument();
    expect(screen.queryByText(/takes the goods off the shelf/)).not.toBeInTheDocument();
  });

  it('stays quiet once the order is past shipping', async () => {
    mockApi('shipped', true);
    renderWithProviders(<AtOrder />, { route: `/sales/orders/${ORDER_ID}` });

    expect(await screen.findByRole('button', { name: 'Mark delivered' })).toBeInTheDocument();
    expect(screen.queryByText(/takes the goods off the shelf/)).not.toBeInTheDocument();
  });
});
