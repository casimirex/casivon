import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes } from 'react-router-dom';
import { renderWithProviders } from '@/test/renderWithProviders';
import { PurchaseOrderDetail } from './PurchaseOrderPages';
import { http } from '@/api/client';
import { setBaseCurrency } from '@/lib/utils';

/// Real UUIDs: the form validates them as such, and 'wh-1' fails silently as
/// "Warehouse is invalid" rather than as anything about the return.
const WAREHOUSE_ID = '11111111-1111-4111-8111-111111111111';
const PO_ID = '22222222-2222-4222-8222-222222222222';
const LINE_ID = '33333333-3333-4333-8333-333333333333';

const WAREHOUSES = [{ id: WAREHOUSE_ID, code: 'MAIN', name: 'Main', is_active: true }];

/// A fully received order — the state most likely to have something wrong with
/// it, and the one `canReceive` alone would have hidden the action on.
function order(overrides: Record<string, unknown> = {}) {
  return {
    id: PO_ID,
    po_number: 'PO-2026-000001',
    vendor_id: 'v-1',
    status: 'fully_received',
    order_date: '2026-03-01',
    currency: 'USD',
    total: '400.00',
    amount_paid: '0.00',
    amount_due: '400.00',
    lines: [
      {
        id: LINE_ID,
        description: 'Widget',
        quantity: 100,
        received_quantity: 100,
        outstanding: 0,
        unit_price: '4.00',
        tax_rate: '0',
        line_total: '400.00',
      },
    ],
    ...overrides,
  };
}

/// The detail page reads its id from the route, so it has to be rendered behind
/// a matching `Route` rather than on its own.
function AtOrder() {
  return (
    <Routes>
      <Route path="/purchasing/purchase-orders/:id" element={<PurchaseOrderDetail />} />
    </Routes>
  );
}

function mockApi(po: Record<string, unknown> = order()) {
  vi.spyOn(http, 'get').mockImplementation(((path: string) =>
    Promise.resolve(path.includes('purchase-orders') ? po : {})) as never);

  vi.spyOn(http, 'list').mockImplementation(((path: string) =>
    Promise.resolve({
      success: true,
      data: path.includes('warehouses') ? WAREHOUSES : [],
      pagination: { page: 1, per_page: 200, total: 1, total_pages: 1 },
    })) as never);
}

describe('sending goods back', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    setBaseCurrency('USD');
  });

  it('offers the action on a fully received order', async () => {
    mockApi();
    renderWithProviders(<AtOrder />, { route: '/purchasing/purchase-orders/po-1' });

    expect(await screen.findByRole('button', { name: /Send back/ })).toBeInTheDocument();
    // Receiving is done, so that action is gone — but returning is not.
    expect(screen.queryByRole('button', { name: /Receive goods/ })).not.toBeInTheDocument();
  });

  it('says nothing about returning when nothing has arrived', async () => {
    mockApi(
      order({
        status: 'confirmed',
        lines: [
          {
            id: LINE_ID,
            description: 'Widget',
            quantity: 100,
            received_quantity: 0,
            outstanding: 100,
            unit_price: '4.00',
            tax_rate: '0',
            line_total: '400.00',
          },
        ],
      })
    );
    renderWithProviders(<AtOrder />, { route: '/purchasing/purchase-orders/po-1' });

    expect(await screen.findByRole('button', { name: /Receive goods/ })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Send back/ })).not.toBeInTheDocument();
  });

  it('starts at zero rather than pre-filling the whole delivery', async () => {
    mockApi();
    renderWithProviders(<AtOrder />, { route: '/purchasing/purchase-orders/po-1' });

    await userEvent.click(await screen.findByRole('button', { name: /Send back/ }));

    // Receiving pre-fills everything outstanding because that is the common
    // case. Returning everything is not, and a form defaulting to the whole
    // delivery is one stray click from a large credit note.
    const dialog = within(screen.getByRole('dialog'));
    expect(dialog.getByRole('spinbutton')).toHaveValue(0);
    // The quantity on the shelf is shown beside it, so the zero is obviously a
    // starting point rather than the whole story.
    expect(dialog.getByText('100')).toBeInTheDocument();
  });

  it('sends what was entered, with the reason', async () => {
    mockApi();
    const post = vi.spyOn(http, 'post').mockResolvedValue({
      id: 'pr-1',
      return_number: 'PR-2026-000001',
      order_status: 'partially_received',
      lines: [],
    } as never);

    renderWithProviders(<AtOrder />, { route: '/purchasing/purchase-orders/po-1' });
    await userEvent.click(await screen.findByRole('button', { name: /Send back/ }));

    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Warehouse' }), WAREHOUSE_ID);
    await userEvent.clear(screen.getByRole('spinbutton'));
    await userEvent.type(screen.getByRole('spinbutton'), '10');
    await userEvent.type(screen.getByPlaceholderText(/Arrived damaged/), 'Wrong colour');

    const dialog = within(screen.getByRole('dialog'));
    await userEvent.click(dialog.getByRole('button', { name: 'Send back' }));

    await waitFor(() =>
      expect(post).toHaveBeenCalledWith(
        '/purchasing/purchase-returns',
        expect.objectContaining({
          po_id: PO_ID,
          warehouse_id: WAREHOUSE_ID,
          reason: 'Wrong colour',
          lines: [{ po_line_id: LINE_ID, quantity_returned: 10 }],
        })
      )
    );
  });

  it('refuses to send back more than arrived, before asking the server', async () => {
    mockApi();
    const post = vi.spyOn(http, 'post').mockResolvedValue({} as never);

    renderWithProviders(<AtOrder />, { route: '/purchasing/purchase-orders/po-1' });
    await userEvent.click(await screen.findByRole('button', { name: /Send back/ }));

    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Warehouse' }), WAREHOUSE_ID);
    await userEvent.clear(screen.getByRole('spinbutton'));
    await userEvent.type(screen.getByRole('spinbutton'), '150');
    const dialog = within(screen.getByRole('dialog'));
    await userEvent.click(dialog.getByRole('button', { name: 'Send back' }));

    expect(await screen.findByText('Only 100 were received')).toBeInTheDocument();
    expect(post).not.toHaveBeenCalled();
  });

  it('will not send an empty return', async () => {
    mockApi();
    const post = vi.spyOn(http, 'post').mockResolvedValue({} as never);

    renderWithProviders(<AtOrder />, { route: '/purchasing/purchase-orders/po-1' });
    await userEvent.click(await screen.findByRole('button', { name: /Send back/ }));

    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Warehouse' }), WAREHOUSE_ID);
    const dialog = within(screen.getByRole('dialog'));
    await userEvent.click(dialog.getByRole('button', { name: 'Send back' }));

    expect(await screen.findByText('Send back at least one item')).toBeInTheDocument();
    expect(post).not.toHaveBeenCalled();
  });
});
