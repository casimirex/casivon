import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen } from '@testing-library/react';
import { Route, Routes } from 'react-router-dom';
import { renderWithProviders } from '@/test/renderWithProviders';
import { InvoiceDetail } from './InvoicePages';
import { http } from '@/api/client';
import { setBaseCurrency } from '@/lib/utils';

const INVOICE_ID = '22222222-2222-4222-8222-222222222222';

/// An invoice with the whole amount outstanding, so the only thing deciding
/// whether it can take money is its status.
function invoice(status: string) {
  return {
    id: INVOICE_ID,
    invoice_number: 'INV-2026-000001',
    customer_id: 'c-1',
    status,
    issue_date: '2026-03-01',
    due_date: '2026-03-31',
    currency: 'USD',
    subtotal: '200.00',
    tax_amount: '0.00',
    total: '200.00',
    amount_paid: '0.00',
    amount_due: '200.00',
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
      },
    ],
    payments: [],
  };
}

function AtInvoice() {
  return (
    <Routes>
      <Route path="/sales/invoices/:id" element={<InvoiceDetail />} />
    </Routes>
  );
}

function mockApi(status: string) {
  vi.spyOn(http, 'get').mockImplementation(((path: string) =>
    Promise.resolve(path.includes('invoices') ? invoice(status) : {})) as never);
  vi.spyOn(http, 'list').mockImplementation((() =>
    Promise.resolve({
      success: true,
      data: [],
      pagination: { page: 1, per_page: 200, total: 0, total_pages: 0 },
    })) as never);
}

describe('recording a payment', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    setBaseCurrency('USD');
  });

  it('is not offered on a draft', async () => {
    mockApi('draft');
    renderWithProviders(<AtInvoice />, { route: `/sales/invoices/${INVOICE_ID}` });

    // A draft has raised no receivable, so there is nothing for money to settle
    // — the server refuses it, and the button should not invite the attempt.
    expect(await screen.findByText('INV-2026-000001')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Record payment/ })).not.toBeInTheDocument();
  });

  it('is offered once the invoice has been issued', async () => {
    mockApi('sent');
    renderWithProviders(<AtInvoice />, { route: `/sales/invoices/${INVOICE_ID}` });

    expect(await screen.findByRole('button', { name: /Record payment/ })).toBeInTheDocument();
  });

  it('is not offered on a cancelled invoice', async () => {
    mockApi('cancelled');
    renderWithProviders(<AtInvoice />, { route: `/sales/invoices/${INVOICE_ID}` });

    expect(await screen.findByText('INV-2026-000001')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Record payment/ })).not.toBeInTheDocument();
  });
});
