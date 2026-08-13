-- Sending goods back to a supplier.
--
-- There has been no return or credit-note concept anywhere in purchasing: an
-- order could be raised, received and paid for, and that was the whole story.
-- Perpetual inventory turned that from untidy into wrong. The only tool for a
-- faulty delivery was a hand-made stock adjustment, which:
--
--   * relieves Inventory and debits **Inventory adjustment** — an expense — so
--     the goods became a permanent loss rather than a credit; and
--   * leaves `amount_due` untouched, because settlement is derived from the
--     vendor payment ledger and a stock movement is not a payment.
--
-- So sending goods back booked a loss *and* left you owing the supplier for
-- stock you no longer had.

CREATE SEQUENCE purchase_return_number_seq;

-- Mirrors `goods_receipts` field for field, because a return is exactly a
-- receipt pointing the other way and anything that reads one should recognise
-- the other.
CREATE TABLE purchase_returns (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    org_id UUID,
    po_id UUID NOT NULL REFERENCES purchase_orders(id),
    return_number VARCHAR(100) NOT NULL UNIQUE,
    return_date DATE NOT NULL,

    -- Which shelf the goods leave from. Nullable for the same reason the
    -- receipt's is: a line that never held stock has no warehouse.
    warehouse_id UUID REFERENCES warehouses(id),

    -- Why they went back. Free text rather than an enum: "wrong colour" and
    -- "arrived damaged" are notes for a human, and a fixed list would be wrong
    -- for somebody within a week.
    reason TEXT,
    notes TEXT,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE purchase_return_lines (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    return_id UUID NOT NULL REFERENCES purchase_returns(id) ON DELETE CASCADE,
    po_line_id UUID NOT NULL REFERENCES purchase_order_lines(id),
    product_id UUID REFERENCES products(id),
    quantity_returned INTEGER NOT NULL,
    notes TEXT,

    CONSTRAINT purchase_return_lines_quantity_positive CHECK (quantity_returned > 0)
);

CREATE INDEX idx_purchase_returns_po ON purchase_returns (po_id);
CREATE INDEX idx_purchase_returns_date ON purchase_returns (return_date);
CREATE INDEX idx_purchase_return_lines_return ON purchase_return_lines (return_id);
CREATE INDEX idx_purchase_return_lines_po_line ON purchase_return_lines (po_line_id);

-- No money is stored on either table, and that is deliberate rather than an
-- omission: a return records **quantities**, and what they are worth is the
-- purchase order's own line price. Valuing it any other way is what would need a
-- variance account — the goods are credited by the supplier at exactly what they
-- were invoiced at, which is exactly what the receipt capitalised, so the debit
-- to payables and the credit to inventory agree by construction.
--
-- Storing a total here would be a second copy of a figure that already exists
-- one join away, free to drift from it.
