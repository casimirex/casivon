-- Crediting a customer: the sales counterpart of a purchase return.
--
-- There has been no credit or refund concept anywhere in sales, and
-- `InvoiceStatus::can_transition` gives `paid` no outgoing transitions at all —
-- a paid invoice is terminal. So a customer who pays and then sends two of ten
-- items back left nothing to do: no credit note, no partial adjustment, and no
-- way to cancel. The only escape was hand-writing a journal entry that ties to
-- nothing — not the invoice, not the stock, not `amount_paid`/`amount_due`.
--
-- Purchasing already runs order → receive → capitalise → return → pay with the
-- ledger and the debt both following along. This is the same for the sell side.

CREATE SEQUENCE credit_note_number_seq;

CREATE TABLE credit_notes (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    org_id UUID,
    credit_note_number VARCHAR(100) NOT NULL UNIQUE,

    -- Always against an invoice. A standing credit with no document behind it is
    -- a different feature, and one nobody has asked for.
    invoice_id UUID NOT NULL REFERENCES invoices(id),
    customer_id UUID NOT NULL REFERENCES companies(id),

    issue_date DATE NOT NULL,

    -- Why it was issued, in whatever words fit. Free text rather than an enum:
    -- "damaged in transit" and "agreed discount" are notes for a human.
    reason TEXT,

    -- Where returned goods land. NULL is ordinary and means no goods came back —
    -- a price dispute or an over-billing credits money and nothing else.
    warehouse_id UUID REFERENCES warehouses(id),

    -- No status column: a credit note is issued and that is the whole of its
    -- life, exactly like a goods receipt or a purchase return.

    subtotal DECIMAL(15, 2) NOT NULL DEFAULT 0,
    tax_amount DECIMAL(15, 2) NOT NULL DEFAULT 0,
    total DECIMAL(15, 2) NOT NULL DEFAULT 0,
    currency VARCHAR(3) NOT NULL DEFAULT 'USD',
    -- The *invoice's* rate, not today's: the receivable was raised at that rate,
    -- and relieving it at any other would leave a difference nothing accounts
    -- for.
    fx_rate DECIMAL(18, 8) NOT NULL DEFAULT 1,
    base_total DECIMAL(15, 2) NOT NULL DEFAULT 0,

    notes TEXT,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT credit_notes_total_is_positive CHECK (total >= 0),
    CONSTRAINT credit_notes_fx_rate_positive CHECK (fx_rate > 0)
);

-- The money *is* stored here, unlike on a purchase return, and for a harder
-- reason than convention. `invoice_entries` derives its revenue leg as
-- `base_total − base_tax` precisely so rounding cannot leave the legs disagreeing
-- with the receivable they created. A credit note has to reverse that
-- identically, which means it needs its own stored total to be the remainder's
-- anchor.

CREATE TABLE credit_note_lines (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    credit_note_id UUID NOT NULL REFERENCES credit_notes(id) ON DELETE CASCADE,

    -- Which invoice line is being credited. Everything about the price comes
    -- from there — a credit is worth what was charged, not what the item is
    -- worth today.
    invoice_line_id UUID NOT NULL REFERENCES invoice_lines(id),
    product_id UUID REFERENCES products(id) ON DELETE SET NULL,

    description TEXT NOT NULL,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    unit_price DECIMAL(15, 2) NOT NULL,
    discount_percent DECIMAL(5, 2) NOT NULL DEFAULT 0,
    tax_rate DECIMAL(5, 2) NOT NULL DEFAULT 0,
    line_total DECIMAL(15, 2) NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_credit_notes_invoice ON credit_notes (invoice_id);
CREATE INDEX idx_credit_notes_customer ON credit_notes (customer_id);
CREATE INDEX idx_credit_notes_date ON credit_notes (issue_date);
CREATE INDEX idx_credit_note_lines_note ON credit_note_lines (credit_note_id);

-- How much of an invoice line has already been credited is asked on every new
-- credit note, so it gets an index. A purchase return needed no equivalent
-- because it decrements `received_quantity` on the order line; invoice lines are
-- immutable, so this tally is the only source of truth.
CREATE INDEX idx_credit_note_lines_invoice_line ON credit_note_lines (invoice_line_id);
