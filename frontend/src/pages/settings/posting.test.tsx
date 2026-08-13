import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '@/test/renderWithProviders';
import { PostingSettings } from './PostingSettings';
import { http } from '@/api/client';
import { setBaseCurrency } from '@/lib/utils';

const ACCOUNTS = [
  { id: 'acc-ar', account_code: '1100', account_name: 'Receivables', account_type: 'asset', is_active: true },
  { id: 'acc-rev', account_code: '4000', account_name: 'Sales', account_type: 'revenue', is_active: true },
  { id: 'acc-old', account_code: '4001', account_name: 'Retired', account_type: 'revenue', is_active: false },
];

/// Routes the GETs this screen makes by path.
///
/// `opening` defaults to periodic costing, which is what every test that is not
/// about stock should see.
function mockApi(config: unknown, unposted: unknown, opening: unknown = PERIODIC) {
  vi.spyOn(http, 'get').mockImplementation(((path: string) => {
    if (path.includes('inventory-opening')) return Promise.resolve(opening);
    return Promise.resolve(path.includes('unposted') ? unposted : config);
  }) as never);

  vi.spyOn(http, 'list').mockResolvedValue({
    success: true,
    data: ACCOUNTS,
    pagination: { page: 1, per_page: 200, total: 3, total_pages: 1 },
  } as never);
}

const NOTHING_UNPOSTED = { posting_enabled: true, documents: [] };

const PERIODIC = {
  perpetual_inventory: false,
  already_posted: false,
  total_value: '0.00',
  lines: [],
  assumes_everything_was_received: 'Credits Cost of sales.',
};

const STOCK_TO_OPEN = {
  perpetual_inventory: true,
  already_posted: false,
  total_value: '675.00',
  lines: [
    { product_id: 'p-1', sku: 'SKU-1', name: 'Widget', quantity: 150, average_cost: '4.5000', value: '675.00' },
  ],
  assumes_everything_was_received:
    'Credits Cost of sales, where goods received under periodic costing were already expensed.',
};

describe('<PostingSettings />', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    setBaseCurrency('USD');
  });

  it('says posting is off and names what is still missing', async () => {
    mockApi(
      {
        accounts: { ar_account_id: 'acc-ar' },
        posting_enabled: false,
        missing_roles: [
          'Bank',
          'Sales revenue',
          'Tax payable',
          'Foreign exchange gain/loss',
          'Accounts payable',
          'Cost of sales',
          'Purchase tax',
          'Employee payable',
          'Employee expense',
        ],
      },
      { posting_enabled: false, documents: [] }
    );

    renderWithProviders(<PostingSettings />);

    expect(await screen.findByText('Posting is off.')).toBeInTheDocument();
    // Naming the gap is the whole job of this banner: silently doing nothing is
    // what the feature is designed to avoid.
    expect(screen.getByText(/Bank, Sales revenue, Tax payable/)).toBeInTheDocument();
  });

  it('confirms when posting is on', async () => {
    mockApi(
      { accounts: { ar_account_id: 'acc-ar' }, posting_enabled: true, missing_roles: [] },
      NOTHING_UNPOSTED
    );

    renderWithProviders(<PostingSettings />);

    expect(await screen.findByText(/Posting is on/)).toBeInTheDocument();
    expect(screen.queryByText('Posting is off.')).not.toBeInTheDocument();
  });

  it('offers only accounts that could actually take the posting', async () => {
    mockApi(
      { accounts: {}, posting_enabled: false, missing_roles: ['Sales revenue'] },
      NOTHING_UNPOSTED
    );

    renderWithProviders(<PostingSettings />);

    const revenue = await screen.findByRole('combobox', { name: /Sales revenue/ });
    const options = Array.from(revenue.querySelectorAll('option')).map((o) => o.textContent);

    // Revenue accounts only — the server refuses anything else, and finding
    // that out by trial and error is a poor way to fill in a form. The retired
    // account is excluded too.
    expect(options).toEqual(['Not set', '4000 — Sales']);
  });

  it('lists what the ledger is owed and offers to post it', async () => {
    mockApi(
      { accounts: {}, posting_enabled: true, missing_roles: [] },
      {
        posting_enabled: true,
        documents: [
          { kind: 'sales_invoice', id: 'inv-1', reference: 'INV-004', date: '2026-03-01', base_amount: '1200.00' },
          { kind: 'goods_receipt', id: 'gr-1', reference: 'GR-002', date: '2026-03-05', base_amount: '600.00' },
        ],
      }
    );

    renderWithProviders(<PostingSettings />);

    expect(await screen.findByText('Invoice')).toBeInTheDocument();
    // Every kind gets a name a person recognises, not a reference_type slug.
    expect(screen.getByText('Goods receipt')).toBeInTheDocument();
    expect(screen.getByText('$1,200.00')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Post 2 document/ })).toBeEnabled();
  });

  it('will not offer to post while there is nowhere to post to', async () => {
    mockApi(
      { accounts: {}, posting_enabled: false, missing_roles: ['Bank'] },
      {
        posting_enabled: false,
        documents: [
          { kind: 'sales_invoice', id: 'inv-1', reference: 'INV-004', date: '2026-03-01', base_amount: '1200.00' },
        ],
      }
    );

    renderWithProviders(<PostingSettings />);

    expect(await screen.findByRole('button', { name: /Post 1 document/ })).toBeDisabled();
    expect(screen.getByText(/Choose the posting accounts first/)).toBeInTheDocument();
  });

  it('offers the stock accounts as an opt-in rather than a requirement', async () => {
    mockApi(
      { accounts: { ar_account_id: 'acc-ar' }, posting_enabled: true, missing_roles: [] },
      NOTHING_UNPOSTED
    );

    renderWithProviders(<PostingSettings />);

    // Posting is on with the two left empty, which is the compatibility promise
    // the whole design turns on.
    expect(await screen.findByText(/Posting is on/)).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Inventory' })).toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Inventory adjustment' })).toBeInTheDocument();
    expect(screen.getByText('Optional')).toBeInTheDocument();
  });

  it('says nothing about opening stock while costing is periodic', async () => {
    mockApi(
      { accounts: { ar_account_id: 'acc-ar' }, posting_enabled: true, missing_roles: [] },
      NOTHING_UNPOSTED
    );

    renderWithProviders(<PostingSettings />);

    await screen.findByText(/Posting is on/);
    // A question nobody asked: without the accounts mapped there is nowhere for
    // an opening balance to go.
    expect(screen.queryByText('Opening stock balance')).not.toBeInTheDocument();
  });

  it('shows what opening the stock balance would post, and the caveat', async () => {
    mockApi(
      { accounts: { ar_account_id: 'acc-ar' }, posting_enabled: true, missing_roles: [] },
      NOTHING_UNPOSTED,
      STOCK_TO_OPEN
    );
    const post = vi.spyOn(http, 'post').mockResolvedValue({
      ...STOCK_TO_OPEN,
      already_posted: true,
    } as never);

    renderWithProviders(<PostingSettings />);

    expect(await screen.findByText('Opening stock balance')).toBeInTheDocument();
    expect(screen.getByText('SKU-1')).toBeInTheDocument();
    expect(screen.getAllByText('$675.00').length).toBeGreaterThan(0);
    // The honest bit, on screen rather than buried in a comment.
    expect(screen.getByText(/already expensed/)).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: /Post \$675.00 to Inventory/ }));
    expect(post).toHaveBeenCalledWith('/accounting/inventory-opening', {});
  });

  it('does not offer to open a balance that is already open', async () => {
    mockApi(
      { accounts: { ar_account_id: 'acc-ar' }, posting_enabled: true, missing_roles: [] },
      NOTHING_UNPOSTED,
      { ...STOCK_TO_OPEN, already_posted: true }
    );

    renderWithProviders(<PostingSettings />);

    expect(await screen.findByText(/Posted\. Stock on hand is on the balance sheet/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /to Inventory/ })).not.toBeInTheDocument();
  });
});
