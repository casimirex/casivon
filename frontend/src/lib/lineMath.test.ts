import { describe, expect, it } from 'vitest';
import { previewLine, sumLines } from './lineMath';
import { formatMoney, humanize, pruneEmpty, toMoneyString } from './utils';

/**
 * The on-screen totals must agree with `shared::money` on the server, or the
 * figure a user approves is not the figure that gets stored.
 */
describe('previewLine', () => {
  it('applies the discount before the tax', () => {
    // 3 x 100 = 300, less 10% = 270, plus 20% tax = 324
    const line = previewLine(3, 100, 10, 20);
    expect(line.net).toBe(270);
    expect(line.tax).toBe(54);
    expect(line.gross).toBe(324);
  });

  it('leaves the amount alone at zero rates', () => {
    expect(previewLine(2, 19.99)).toEqual({ net: 39.98, tax: 0, gross: 39.98 });
  });

  it('zeroes the line at a full discount', () => {
    expect(previewLine(5, 10, 100, 20).net).toBe(0);
  });

  it('treats empty inputs as zero rather than NaN', () => {
    expect(previewLine(Number('') , Number('nope'))).toEqual({ net: 0, tax: 0, gross: 0 });
  });

  it('rounds to cents', () => {
    expect(previewLine(3, 0.335).net).toBe(1.01);
  });
});

describe('sumLines', () => {
  it('totals a mixed document', () => {
    const totals = sumLines([
      { quantity: 2, unit_price: 50, tax_rate: 10 },
      { quantity: 1, unit_price: 25, discount_percent: 20, tax_rate: 10 },
    ]);
    expect(totals.subtotal).toBe(120);
    expect(totals.tax).toBe(12);
    expect(totals.total).toBe(132);
  });

  it('handles the values react-hook-form gives back as strings', () => {
    const totals = sumLines([{ quantity: '2', unit_price: '10.50', tax_rate: '20' }]);
    expect(totals.subtotal).toBe(21);
    expect(totals.tax).toBe(4.2);
    expect(totals.total).toBe(25.2);
  });

  it('is zero for an empty document', () => {
    expect(sumLines([])).toEqual({ subtotal: 0, tax: 0, total: 0 });
  });
});

describe('formatMoney', () => {
  it('formats the strings the API sends', () => {
    expect(formatMoney('1080.00')).toBe('$1,080.00');
  });

  it('shows an em dash rather than NaN for missing values', () => {
    expect(formatMoney(null)).toBe('—');
    expect(formatMoney(undefined)).toBe('—');
    expect(formatMoney('')).toBe('—');
  });

  it('respects the document currency', () => {
    expect(formatMoney('50.00', 'EUR')).toBe('€50.00');
  });
});

describe('toMoneyString', () => {
  it('fixes the scale for the wire', () => {
    expect(toMoneyString(19.9)).toBe('19.90');
    expect(toMoneyString('5')).toBe('5.00');
  });

  it('returns nothing for a blank field', () => {
    expect(toMoneyString('')).toBeUndefined();
    expect(toMoneyString(null)).toBeUndefined();
  });
});

describe('humanize', () => {
  it('turns a status key into a label', () => {
    expect(humanize('partially_received')).toBe('Partially received');
    expect(humanize(null)).toBe('—');
  });
});

describe('pruneEmpty', () => {
  it('keeps false and zero, which are real values', () => {
    expect(pruneEmpty({ active: false, count: 0, note: '', missing: undefined })).toEqual({
      active: false,
      count: 0,
    });
  });
});
