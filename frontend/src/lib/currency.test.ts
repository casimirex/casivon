import { describe, expect, it, afterEach } from 'vitest';
import { formatMoney, getBaseCurrency, setBaseCurrency } from './utils';

/**
 * `formatMoney` used to default to USD regardless of what the organisation was
 * configured for, so a EUR installation showed dollar signs on every screen.
 */
describe('base currency', () => {
  afterEach(() => setBaseCurrency('USD'));

  it('falls back to the seeded default before the profile loads', () => {
    expect(getBaseCurrency()).toBe('USD');
    expect(formatMoney('1080.00')).toContain('$');
  });

  it('labels amounts with the organisation currency once it is known', () => {
    setBaseCurrency('EUR');

    expect(getBaseCurrency()).toBe('EUR');
    expect(formatMoney('1080.00')).toContain('€');
    // The number itself is untouched — this is a label, not a conversion.
    expect(formatMoney('1080.00')).toContain('1,080.00');
  });

  it('normalises the code, since the API stores it upper case', () => {
    setBaseCurrency('gbp');
    expect(getBaseCurrency()).toBe('GBP');
    expect(formatMoney('10.00')).toContain('£');
  });

  it('ignores an empty code rather than producing an invalid format', () => {
    setBaseCurrency('EUR');
    setBaseCurrency('');
    expect(getBaseCurrency()).toBe('EUR');
  });

  it('still honours an explicitly passed currency', () => {
    setBaseCurrency('EUR');
    expect(formatMoney('10.00', 'JPY')).toContain('¥');
  });
});
