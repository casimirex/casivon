import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '@/test/renderWithProviders';
import { CurrencyField } from './CurrencyField';
import { DocumentLines } from './DocumentView';
import { http } from '@/api/client';
import { setBaseCurrency } from '@/lib/utils';

/** Stubs `GET /settings/currencies`, which the picker reads its options from. */
function mockCurrencies(base: string, available: string[]) {
  return vi.spyOn(http, 'get').mockResolvedValue({ base, available });
}

describe('<CurrencyField />', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    setBaseCurrency('USD');
  });

  it('offers every currency the server will accept', async () => {
    mockCurrencies('USD', ['EUR', 'GBP', 'USD']);
    renderWithProviders(<CurrencyField />);

    const select = await screen.findByRole('combobox', { name: 'Currency' });
    const options = Array.from(select.querySelectorAll('option')).map((o) => o.textContent);
    expect(options).toEqual(['EUR', 'GBP', 'USD']);
  });

  it('is a read-only label while there is only one currency to pick', async () => {
    mockCurrencies('USD', ['USD']);
    renderWithProviders(<CurrencyField />);

    // A dropdown holding a single option is a worse control than a label, and
    // the hint has to say what would make a second one available.
    const field = await screen.findByLabelText('Currency');
    expect(field).toBeDisabled();
    expect(field).toHaveValue('USD');
    expect(screen.queryByRole('combobox')).not.toBeInTheDocument();
    expect(screen.getByText(/Exchange rates/i)).toBeInTheDocument();
  });

  it('reports the chosen currency to the form', async () => {
    mockCurrencies('USD', ['EUR', 'USD']);
    const onChange = vi.fn();
    renderWithProviders(<CurrencyField name="currency" onChange={onChange} />);

    const select = await screen.findByRole('combobox', { name: 'Currency' });
    await userEvent.selectOptions(select, 'EUR');

    expect(onChange).toHaveBeenCalled();
    expect(select).toHaveValue('EUR');
  });
});

describe('<DocumentLines /> base equivalent', () => {
  beforeEach(() => setBaseCurrency('USD'));

  const lines = [
    {
      id: 'line-1',
      description: 'Widget',
      quantity: 1,
      unit_price: '100.00',
      discount_percent: '0.00',
      tax_rate: '0.00',
      line_total: '100.00',
    },
  ];

  it('shows what a foreign document is worth in the base currency', () => {
    renderWithProviders(
      <DocumentLines
        lines={lines as never}
        currency="EUR"
        subtotal="100.00"
        tax="0.00"
        total="100.00"
        baseTotal="110.00"
        fxRate="1.10"
      />
    );

    // The customer's figure and the business's figure, both on screen, with the
    // rate that connects them — so nobody has to guess which is which. The euro
    // total appears several times (line, subtotal, total); the restated one is
    // shown exactly once, under the total.
    expect(screen.getAllByText('€100.00').length).toBeGreaterThan(0);
    expect(screen.getByText('$110.00')).toBeInTheDocument();
    expect(screen.getByText(/In USD at 1.10/)).toBeInTheDocument();
  });

  it('says nothing extra when the document is already in the base currency', () => {
    renderWithProviders(
      <DocumentLines
        lines={lines as never}
        currency="USD"
        subtotal="100.00"
        tax="0.00"
        total="100.00"
        baseTotal="100.00"
        fxRate="1"
      />
    );

    // Repeating the same number underneath itself reads as a mistake.
    expect(screen.queryByText(/In USD at/)).not.toBeInTheDocument();
  });
});
