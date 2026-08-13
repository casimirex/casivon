import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { Route, Routes } from 'react-router-dom';
import { renderWithProviders } from '@/test/renderWithProviders';
import { InvoiceDetail } from './InvoicePages';
import { http } from '@/api/client';
import { setBaseCurrency } from '@/lib/utils';

/// Real UUIDs: the form validates the warehouse as one, and a placeholder fails
/// as "Warehouse is invalid" rather than as anything about the credit.
const WAREHOUSE_ID = '11111111-1111-4111-8111-111111111111';
const INVOICE_ID = '22222222-2222-4222-8222-222222222222';
const LINE_ID = '33333333-3333-4333-8333-333333333333';

const WAREHOUSES = [{ id: WAREHOUSE_ID, code: 'MAIN', name: 'Main', is_active: true }];

/// A **paid** invoice — the state that had no answer before credit notes: `paid`
/// has no outgoing status transition, so it could be neither cancelled nor
/// adjusted.
function invoice(overrides: Record<string, unknown> = {}) {
  return {
    id: INVOICE_ID,
    invoice_number: 'INV-2026-000001',
    customer_id: 'c-1',
    status: 'paid',
    issue_date: '2026-03-01',
    due_date: '2026-03-31',
    currency: 'USD',
    subtotal: '200.00',
    tax_amount: '0.00',
    total: '200.00',
    amount_paid: '200.00',
    amount_due: '0.00',
    lines: [
      {
        id: LINE_ID,
        description: 'Widget',
        quantity: 10,
        unit_price: '20.00',
        discount_percent: '0',
        tax_rate: '0',
        line_total: '200.00',
        sort_order: 0,
      },
    ],
    payments: [],
    ...overrides,
  };
}

function AtInvoice() {
  return (
    <Routes>
      <Route path="/sales/invoices/:id" element={<InvoiceDetail />} />
    </Routes>
  );
}

function mockApi(doc: Record<string, unknown> = invoice()) {
  vi.spyOn(http, 'get').mockImplementation(((path: string) =>
    Promise.resolve(path.includes('invoices') ? doc : {})) as never);

  vi.spyOn(http, 'list').mockImplementation(((path: string) =>
    Promise.resolve({
      success: true,
      data: path.includes('warehouses') ? WAREHOUSES : [],
      pagination: { page: 1, per_page: 200, total: 1, total_pages: 1 },
    })) as never);
}

describe('issuing a credit note', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    setBaseCurrency('USD');
  });

  it('is offered on a paid invoice, which has no other way out', async () => {
    mockApi();
    renderWithProviders(<AtInvoice />, { route: `/sales/invoices/${INVOICE_ID}` });

    expect(await screen.findByRole('button', { name: /Credit note/ })).toBeInTheDocument();
    // Nothing is owed, so there is no payment to record — crediting is the only
    // action left, and before this there was none.
    expect(screen.queryByRole('button', { name: /Record payment/ })).not.toBeInTheDocument();
  });

  it('is not offered on a draft, which has no receivable to relieve', async () => {
    mockApi(invoice({ status: 'draft', amount_paid: '0.00', amount_due: '200.00' }));
    renderWithProviders(<AtInvoice />, { route: `/sales/invoices/${INVOICE_ID}` });

    await screen.findByText('INV-2026-000001');
    expect(screen.queryByRole('button', { name: /Credit note/ })).not.toBeInTheDocument();
  });

  it('starts at zero, and defaults to crediting money only', async () => {
    mockApi();
    renderWithProviders(<AtInvoice />, { route: `/sales/invoices/${INVOICE_ID}` });
    await userEvent.click(await screen.findByRole('button', { name: /Credit note/ }));

    const dialog = within(screen.getByRole('dialog'));
    // Crediting a whole invoice is not the common case, and a form defaulting to
    // it is one stray click from giving away the sale.
    expect(dialog.getByRole('spinbutton')).toHaveValue(0);
    // No warehouse means no goods came back — a price dispute, not a return.
    expect(dialog.getByRole('combobox', { name: 'Warehouse' })).toHaveValue('');
  });

  it('credits money only when no warehouse is chosen', async () => {
    mockApi();
    const post = vi.spyOn(http, 'post').mockResolvedValue({
      id: 'cn-1',
      credit_note_number: 'CN-2026-000001',
      lines: [],
    } as never);

    renderWithProviders(<AtInvoice />, { route: `/sales/invoices/${INVOICE_ID}` });
    await userEvent.click(await screen.findByRole('button', { name: /Credit note/ }));

    await userEvent.clear(screen.getByRole('spinbutton'));
    await userEvent.type(screen.getByRole('spinbutton'), '2');
    await userEvent.type(screen.getByPlaceholderText(/Returned by the customer/), 'Agreed discount');

    const dialog = within(screen.getByRole('dialog'));
    await userEvent.click(dialog.getByRole('button', { name: 'Issue credit note' }));

    await waitFor(() =>
      expect(post).toHaveBeenCalledWith('/sales/credit-notes', {
        invoice_id: INVOICE_ID,
        warehouse_id: undefined,
        issue_date: expect.any(String),
        reason: 'Agreed discount',
        notes: undefined,
        lines: [{ invoice_line_id: LINE_ID, quantity: 2 }],
      })
    );
  });

  it('sends the warehouse when the goods came back', async () => {
    mockApi();
    const post = vi.spyOn(http, 'post').mockResolvedValue({ lines: [] } as never);

    renderWithProviders(<AtInvoice />, { route: `/sales/invoices/${INVOICE_ID}` });
    await userEvent.click(await screen.findByRole('button', { name: /Credit note/ }));

    await userEvent.selectOptions(
      screen.getByRole('combobox', { name: 'Warehouse' }),
      WAREHOUSE_ID
    );
    await userEvent.clear(screen.getByRole('spinbutton'));
    await userEvent.type(screen.getByRole('spinbutton'), '2');

    const dialog = within(screen.getByRole('dialog'));
    await userEvent.click(dialog.getByRole('button', { name: 'Issue credit note' }));

    await waitFor(() =>
      expect(post).toHaveBeenCalledWith(
        '/sales/credit-notes',
        expect.objectContaining({ warehouse_id: WAREHOUSE_ID })
      )
    );
  });

  it('refuses to credit more than was invoiced, before asking the server', async () => {
    mockApi();
    const post = vi.spyOn(http, 'post').mockResolvedValue({} as never);

    renderWithProviders(<AtInvoice />, { route: `/sales/invoices/${INVOICE_ID}` });
    await userEvent.click(await screen.findByRole('button', { name: /Credit note/ }));

    await userEvent.clear(screen.getByRole('spinbutton'));
    await userEvent.type(screen.getByRole('spinbutton'), '15');

    const dialog = within(screen.getByRole('dialog'));
    await userEvent.click(dialog.getByRole('button', { name: 'Issue credit note' }));

    expect(await screen.findByText('Only 10 left to credit')).toBeInTheDocument();
    expect(post).not.toHaveBeenCalled();
  });

  it('will not issue an empty credit note', async () => {
    mockApi();
    const post = vi.spyOn(http, 'post').mockResolvedValue({} as never);

    renderWithProviders(<AtInvoice />, { route: `/sales/invoices/${INVOICE_ID}` });
    await userEvent.click(await screen.findByRole('button', { name: /Credit note/ }));

    const dialog = within(screen.getByRole('dialog'));
    await userEvent.click(dialog.getByRole('button', { name: 'Issue credit note' }));

    expect(await screen.findByText('Credit at least one item')).toBeInTheDocument();
    expect(post).not.toHaveBeenCalled();
  });
});
