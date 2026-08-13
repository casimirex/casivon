import { describe, expect, it } from 'vitest';
import { toPayload } from './common';
import {
  contactSchema,
  goodsReceiptSchema,
  ledgerEntrySchema,
  movementSchema,
  paymentSchema,
  quoteSchema,
  registerSchema,
  taxRateSchema,
  timeEntrySchema,
} from './index';

/**
 * These assert that the browser rejects exactly what the Rust DTOs reject, so a
 * user is told which field is wrong instead of eating a 422 from the server.
 */

describe('contactSchema', () => {
  const valid = { first_name: 'Ada', last_name: 'Lovelace', status: 'lead' as const };

  it('accepts the minimum a contact needs', () => {
    expect(contactSchema.safeParse(valid).success).toBe(true);
  });

  it('requires both names', () => {
    const result = contactSchema.safeParse({ ...valid, first_name: '' });
    expect(result.success).toBe(false);
    expect(result.error?.issues[0].message).toBe('First name is required');
  });

  it('rejects a malformed email but allows none at all', () => {
    expect(contactSchema.safeParse({ ...valid, email: 'not-an-email' }).success).toBe(false);
    expect(contactSchema.safeParse({ ...valid, email: '' }).success).toBe(true);
  });

  it('drops an empty email rather than sending a blank string', () => {
    const parsed = contactSchema.parse({ ...valid, email: '' });
    expect(parsed.email).toBeUndefined();
  });

  it('rejects a status outside the allowed set', () => {
    expect(contactSchema.safeParse({ ...valid, status: 'vip' }).success).toBe(false);
  });
});

describe('quoteSchema', () => {
  const line = { description: 'Widget', quantity: 1, unit_price: 10 };
  const valid = {
    customer_id: '3f2504e0-4f89-41d3-9a0c-0305e82c3301',
    issue_date: '2026-08-01',
    expiry_date: '2026-09-01',
    lines: [line],
  };

  it('accepts a well-formed quote', () => {
    expect(quoteSchema.safeParse(valid).success).toBe(true);
  });

  it('needs at least one line item', () => {
    const result = quoteSchema.safeParse({ ...valid, lines: [] });
    expect(result.success).toBe(false);
    expect(result.error?.issues[0].message).toBe('Add at least one line item');
  });

  it('refuses an expiry before the issue date, and says which field', () => {
    const result = quoteSchema.safeParse({ ...valid, expiry_date: '2026-07-01' });
    expect(result.success).toBe(false);
    expect(result.error?.issues[0].path).toEqual(['expiry_date']);
  });

  it('rejects a non-UUID customer', () => {
    expect(quoteSchema.safeParse({ ...valid, customer_id: 'globex' }).success).toBe(false);
  });

  it('rejects a zero-quantity line', () => {
    expect(quoteSchema.safeParse({ ...valid, lines: [{ ...line, quantity: 0 }] }).success).toBe(
      false
    );
  });
});

describe('paymentSchema', () => {
  const valid = {
    amount: 50,
    payment_method: 'bank_transfer' as const,
    payment_date: '2026-08-05',
  };

  it('caps the amount at the outstanding balance', () => {
    expect(paymentSchema(100).safeParse(valid).success).toBe(true);

    const result = paymentSchema(20).safeParse(valid);
    expect(result.success).toBe(false);
    expect(result.error?.issues[0].message).toContain('20.00 still outstanding');
  });

  it('rejects a zero payment', () => {
    expect(paymentSchema(100).safeParse({ ...valid, amount: 0 }).success).toBe(false);
  });

  it('rejects a payment method the server does not know', () => {
    expect(paymentSchema(100).safeParse({ ...valid, payment_method: 'crypto' }).success).toBe(
      false
    );
  });
});

describe('movementSchema', () => {
  const warehouse = '3f2504e0-4f89-41d3-9a0c-0305e82c3301';
  const destination = '9c858901-8a57-4791-81fe-4c455b099bc9';
  const base = {
    product_id: '16fd2706-8baf-433b-82eb-8c7fada847da',
    warehouse_id: warehouse,
    movement_type: 'in' as const,
    quantity: 5,
  };

  it('accepts a simple stock-in', () => {
    expect(movementSchema.safeParse(base).success).toBe(true);
  });

  it('rejects a zero-quantity movement', () => {
    expect(movementSchema.safeParse({ ...base, quantity: 0 }).success).toBe(false);
  });

  it('only lets an adjustment go negative', () => {
    expect(movementSchema.safeParse({ ...base, quantity: -3 }).success).toBe(false);
    expect(
      movementSchema.safeParse({ ...base, movement_type: 'adjustment', quantity: -3 }).success
    ).toBe(true);
  });

  it('demands a destination for a transfer', () => {
    const result = movementSchema.safeParse({ ...base, movement_type: 'transfer' });
    expect(result.success).toBe(false);
    expect(result.error?.issues[0].path).toEqual(['to_warehouse_id']);
  });

  it('refuses a transfer to the same warehouse', () => {
    const result = movementSchema.safeParse({
      ...base,
      movement_type: 'transfer',
      to_warehouse_id: warehouse,
    });
    expect(result.success).toBe(false);
    expect(result.error?.issues[0].message).toBe('Source and destination must differ');
  });

  it('accepts a transfer between two different warehouses', () => {
    expect(
      movementSchema.safeParse({
        ...base,
        movement_type: 'transfer',
        to_warehouse_id: destination,
      }).success
    ).toBe(true);
  });
});

describe('ledgerEntrySchema', () => {
  const debit = '3f2504e0-4f89-41d3-9a0c-0305e82c3301';
  const credit = '9c858901-8a57-4791-81fe-4c455b099bc9';
  const valid = {
    entry_date: '2026-08-01',
    description: 'Widget sale',
    debit_account_id: debit,
    credit_account_id: credit,
    amount: 100,
  };

  it('accepts a balanced entry', () => {
    expect(ledgerEntrySchema.safeParse(valid).success).toBe(true);
  });

  it('refuses to debit and credit the same account', () => {
    const result = ledgerEntrySchema.safeParse({ ...valid, credit_account_id: debit });
    expect(result.success).toBe(false);
    expect(result.error?.issues[0].message).toBe('Debit and credit must be different accounts');
  });

  it('rejects a zero amount', () => {
    expect(ledgerEntrySchema.safeParse({ ...valid, amount: 0 }).success).toBe(false);
  });
});

describe('taxRateSchema', () => {
  const valid = { name: 'Standard VAT', rate: 20, tax_type: 'vat' };

  it('accepts a whole percentage, the same convention as a document line', () => {
    expect(taxRateSchema.safeParse(valid).success).toBe(true);
    expect(taxRateSchema.safeParse({ ...valid, rate: 17.5 }).success).toBe(true);
    expect(taxRateSchema.safeParse({ ...valid, rate: 0 }).success).toBe(true);
  });

  it('rejects a rate beyond 100%', () => {
    const result = taxRateSchema.safeParse({ ...valid, rate: 2000 });
    expect(result.success).toBe(false);
    expect(result.error?.issues[0].message).toBe('Rate cannot exceed 100');
  });
});

describe('timeEntrySchema', () => {
  const valid = {
    task_id: '3f2504e0-4f89-41d3-9a0c-0305e82c3301',
    employee_id: '9c858901-8a57-4791-81fe-4c455b099bc9',
    entry_date: '2026-08-03',
    hours: 7.5,
  };

  it('accepts a normal working day', () => {
    expect(timeEntrySchema.safeParse(valid).success).toBe(true);
  });

  it('refuses more than 24 hours in one entry', () => {
    expect(timeEntrySchema.safeParse({ ...valid, hours: 25 }).success).toBe(false);
  });

  it('refuses zero hours', () => {
    expect(timeEntrySchema.safeParse({ ...valid, hours: 0 }).success).toBe(false);
  });
});

describe('goodsReceiptSchema', () => {
  const warehouse = '3f2504e0-4f89-41d3-9a0c-0305e82c3301';
  const line = {
    po_line_id: '9c858901-8a57-4791-81fe-4c455b099bc9',
    description: 'Bolts',
    outstanding: 10,
  };

  it('accepts a partial receipt', () => {
    const result = goodsReceiptSchema.safeParse({
      warehouse_id: warehouse,
      lines: [{ ...line, quantity_received: 4 }],
    });
    expect(result.success).toBe(true);
  });

  it('refuses to receive more than is outstanding', () => {
    const result = goodsReceiptSchema.safeParse({
      warehouse_id: warehouse,
      lines: [{ ...line, quantity_received: 11 }],
    });
    expect(result.success).toBe(false);
    expect(result.error?.issues[0].message).toBe('Only 10 still outstanding');
  });

  it('refuses a receipt where nothing was actually received', () => {
    const result = goodsReceiptSchema.safeParse({
      warehouse_id: warehouse,
      lines: [{ ...line, quantity_received: 0 }],
    });
    expect(result.success).toBe(false);
    expect(result.error?.issues[0].message).toBe('Receive at least one item');
  });
});

describe('registerSchema', () => {
  it('enforces the same 8-character minimum as the server', () => {
    const base = { first_name: 'Ada', last_name: 'Admin', email: 'ada@erp.test' };
    expect(registerSchema.safeParse({ ...base, password: 'short' }).success).toBe(false);
    expect(registerSchema.safeParse({ ...base, password: 'longenough' }).success).toBe(true);
  });
});

describe('toPayload', () => {
  it('serialises money fields to fixed-scale strings', () => {
    const payload = toPayload({ name: 'Widget', sale_price: 19.9 }, ['sale_price']);
    expect(payload).toEqual({ name: 'Widget', sale_price: '19.90' });
  });

  it('drops blanks so an update does not clear untouched fields', () => {
    const payload = toPayload({ name: 'Widget', notes: '', phone: undefined });
    expect(payload).toEqual({ name: 'Widget' });
  });

  it('keeps non-money numbers as numbers', () => {
    const payload = toPayload({ quantity: 3, price: 5 }, ['price']);
    expect(payload).toEqual({ quantity: 3, price: '5.00' });
  });
});
