-- The other half of the books: what the business spends.
--
-- Migration 014 made sales post itself, and in doing so made the profit and
-- loss actively misleading: it reports revenue with no cost against it, so every
-- sale looks like pure profit. A blank report is obviously broken and nobody
-- acts on it; one showing 100% margin looks finished and is wrong in the
-- flattering direction.
--
-- This adds what the spending side needs: somewhere for a payable to live, a way
-- to settle it, and the accounts the new postings use.

-- ---------------------------------------------------------- vendor payments
--
-- Purchasing could raise an order and receive goods, but nothing in the system
-- could ever pay for them. Posting a payable without this would have swapped an
-- overstated P&L for an overstated balance sheet — a liability that only grows.
--
-- Deliberately shaped like `payments` on the sales side, down to the column
-- names: the two are the same idea pointing in opposite directions, and a
-- reader who understands one should not have to learn a second vocabulary. The
-- FX columns carry the same meaning too — see `013_multi_currency.sql`.
CREATE TABLE vendor_payments (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    org_id UUID,
    po_id UUID NOT NULL REFERENCES purchase_orders(id),
    amount DECIMAL(15, 2) NOT NULL,
    currency VARCHAR(3) NOT NULL DEFAULT 'USD',
    -- The rate on the day the money left, which is deliberately not the order's.
    fx_rate DECIMAL(18, 8) NOT NULL DEFAULT 1,
    base_amount DECIMAL(15, 2) NOT NULL,
    -- Positive is a gain: the payment cost less in base currency than the order
    -- committed to. Zero whenever both rates agree.
    fx_gain_loss DECIMAL(15, 2) NOT NULL DEFAULT 0,
    payment_method VARCHAR(50) NOT NULL,
    payment_date DATE NOT NULL,
    reference VARCHAR(255),
    notes TEXT,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT vendor_payments_amount_is_positive CHECK (amount > 0),
    CONSTRAINT vendor_payments_fx_rate_positive CHECK (fx_rate > 0)
);

CREATE INDEX idx_vendor_payments_po ON vendor_payments (po_id);
CREATE INDEX idx_vendor_payments_date ON vendor_payments (payment_date);

-- ------------------------------------------------ purchase order settlement
--
-- The same four columns invoices carry, for the same reason: what is paid and
-- what is still owed are re-derived from the payments recorded against the
-- order, so the order can never drift away from its own payment ledger.
--
-- `base_amount_paid` and `base_amount_due` are restated at the *order's* rate,
-- so the two always reconcile against its base total. What each payment was
-- actually worth lives on the payment, along with the difference between them.
ALTER TABLE purchase_orders
    ADD COLUMN amount_paid DECIMAL(15, 2) NOT NULL DEFAULT 0,
    ADD COLUMN amount_due DECIMAL(15, 2),
    ADD COLUMN base_amount_paid DECIMAL(15, 2) NOT NULL DEFAULT 0,
    ADD COLUMN base_amount_due DECIMAL(15, 2);

-- Nothing has ever been paid, because there was no way to pay it. So every
-- existing order owes its full total, and this backfill is exact rather than an
-- approximation.
UPDATE purchase_orders
SET amount_due      = total,
    base_amount_due = base_total;

-- ---------------------------------------------------------- posting accounts
--
-- Five more roles, taking the mapping from five to ten. Same rules as the
-- others: nullable, ON DELETE RESTRICT, and the required account type enforced
-- in the use case where the referenced row can actually be read.
--
-- Note the consequence for an installation that already configured the sales
-- five: **posting stops** until these are chosen too. Posting is all-or-nothing
-- on a complete mapping, because a partial mapping posts lopsided entries, and
-- that is worse than posting nothing. The settings screen names what is missing.
ALTER TABLE organization_settings
    -- What the business owes suppliers. Credited by a goods receipt, cleared by
    -- a vendor payment.
    ADD COLUMN accounts_payable_account_id  UUID REFERENCES accounts(id) ON DELETE RESTRICT,
    -- Where the cost of received goods lands. Periodic rather than perpetual:
    -- cost reaches the P&L when goods arrive, not when they are sold, because
    -- costing goods out on sale needs a FIFO or average-cost layer that does not
    -- exist. See the plan note in the README.
    ADD COLUMN cost_of_sales_account_id     UUID REFERENCES accounts(id) ON DELETE RESTRICT,
    -- Input tax, split out rather than buried in cost. It is usually
    -- recoverable, so folding it into cost would overstate cost and lose the
    -- reclaim. An asset for the same reason receivables are.
    ADD COLUMN purchase_tax_account_id      UUID REFERENCES accounts(id) ON DELETE RESTRICT,
    -- What the business owes its own staff. Kept apart from suppliers because
    -- the two are different debts and are reported separately.
    ADD COLUMN employee_payable_account_id  UUID REFERENCES accounts(id) ON DELETE RESTRICT,
    ADD COLUMN employee_expense_account_id  UUID REFERENCES accounts(id) ON DELETE RESTRICT;

-- ----------------------------------------------------------------- no backfill
--
-- As with 014: goods receipts, and expense reports already approved, stay
-- unposted rather than being written into the books by a migration. They appear
-- under `GET /accounting/unposted` and are posted when somebody chooses to.
