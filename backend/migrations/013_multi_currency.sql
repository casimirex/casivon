-- Multi-currency: a rate per document, and a base-currency amount beside every
-- figure that anything ever adds up.
--
-- Until now a second currency was refused outright, because every total in the
-- system is a bare `SUM(amount)` and those sums have no idea what currency the
-- rows are in. Ten EUR invoices and ten USD invoices would have produced one
-- number that meant nothing. That refusal was honest, but it capped the product
-- at single-currency.
--
-- The shape that fixes it is the one every accounting system converges on:
--
--   * a document keeps the currency it was *transacted* in, and the amounts the
--     customer actually sees — those must never change,
--   * plus the exchange rate used, frozen onto the row at the moment it was
--     raised,
--   * plus the same amount restated in the organisation's base currency.
--
-- Reports and dashboards then sum the base column and are correct by
-- construction. The transaction amount stays untouched for anything that has to
-- face the customer.
--
-- ---------------------------------------------------------------- rate table

-- `rate` is how many units of the base currency one unit of `currency` buys, so
-- restating an amount is always a multiplication and never a division. With a
-- base of USD, a EUR rate of 1.08 means EUR 100 is USD 108.
--
-- Eight decimal places because currencies like IDR and VND trade at rates with
-- several leading zeros, and because a rate is an input to a multiplication —
-- rounding it early puts the error into every amount derived from it.
--
-- Effective-dated rather than a single current value: an invoice raised in March
-- must keep restating at March's rate forever, and a rate entered today must not
-- silently rewrite last quarter's revenue. The rate in force on a date is the
-- most recent row on or before it.
CREATE TABLE fx_rates (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    currency CHAR(3) NOT NULL,
    effective_from DATE NOT NULL,
    rate DECIMAL(18, 8) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT fx_rates_rate_is_positive CHECK (rate > 0),
    -- One rate per currency per day. Correcting a rate is an update, not a
    -- second row that a lookup would have to break a tie between.
    CONSTRAINT fx_rates_one_per_day UNIQUE (currency, effective_from)
);

-- The lookup is always "greatest effective_from <= $date for this currency".
CREATE INDEX idx_fx_rates_lookup ON fx_rates (currency, effective_from DESC);

CREATE TRIGGER update_fx_rates_updated_at BEFORE UPDATE ON fx_rates
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- The base currency itself never gets a row: its rate is 1 by definition, and a
-- stored 1.0 that someone could edit to 0.98 would silently rescale every
-- document in the system. `organization_settings.default_currency` remains the
-- one place the base currency is named, and the guard that stops it changing
-- once financial documents exist now protects these rates too — every rate here
-- is expressed *against* that currency, so changing it would invalidate all of
-- them at once.

-- ------------------------------------------------- rate + base amount columns
--
-- `fx_rate` is frozen onto each document when it is raised. Storing it, rather
-- than looking it up again at read time, is what makes a historical document
-- reproducible: re-reading would restate a closed invoice at today's rate every
-- time somebody opened it.
--
-- The base amount is stored too, not computed as `amount * fx_rate` on the fly.
-- Two reasons. Rounding: `SUM(ROUND(a * r))` and `ROUND(SUM(a * r))` disagree,
-- and the figure that has to reconcile against the ledger is the per-document
-- rounded one, so it gets computed once and kept. And permanence: a base amount
-- is a posted fact, so a correction to a rate must not retroactively move
-- revenue that was already reported.
--
-- Only figures that something aggregates or reports get a base column. Anything
-- else stays derivable from the row's own `fx_rate`, which is why every table
-- here gets the rate even when it gets only one base amount.

-- Sales pipeline. `value` is what the deal is worth to the customer; the CRM
-- dashboard groups pipeline by stage and has to add those together.
ALTER TABLE opportunities
    ADD COLUMN fx_rate DECIMAL(18, 8) NOT NULL DEFAULT 1,
    ADD COLUMN base_value DECIMAL(15, 2);

ALTER TABLE quotes
    ADD COLUMN fx_rate DECIMAL(18, 8) NOT NULL DEFAULT 1,
    ADD COLUMN base_total DECIMAL(15, 2);

ALTER TABLE sales_orders
    ADD COLUMN fx_rate DECIMAL(18, 8) NOT NULL DEFAULT 1,
    ADD COLUMN base_total DECIMAL(15, 2);

-- Invoices carry three: the total is revenue, and paid/due are what aged
-- receivables reports add up across customers who may not share a currency.
ALTER TABLE invoices
    ADD COLUMN fx_rate DECIMAL(18, 8) NOT NULL DEFAULT 1,
    ADD COLUMN base_total DECIMAL(15, 2),
    ADD COLUMN base_amount_paid DECIMAL(15, 2) DEFAULT 0,
    ADD COLUMN base_amount_due DECIMAL(15, 2);

-- A payment gets its own rate, deliberately different from the invoice's: it is
-- the rate on the day the money actually arrived. The gap between the two is
-- the realised FX gain or loss — an invoice for EUR 1,000 raised at 1.10 and
-- settled at 1.15 brought in USD 50 more than the revenue that was booked, and
-- that difference is a real result that belongs on the payment, in base
-- currency. Zero whenever invoice and payment rates agree, which is always the
-- case in a single-currency installation.
ALTER TABLE payments
    ADD COLUMN fx_rate DECIMAL(18, 8) NOT NULL DEFAULT 1,
    ADD COLUMN base_amount DECIMAL(15, 2),
    ADD COLUMN fx_gain_loss DECIMAL(15, 2) NOT NULL DEFAULT 0;

ALTER TABLE purchase_orders
    ADD COLUMN fx_rate DECIMAL(18, 8) NOT NULL DEFAULT 1,
    ADD COLUMN base_total DECIMAL(15, 2);

-- The trial balance and every account balance are sums over this table.
ALTER TABLE general_ledger_entries
    ADD COLUMN fx_rate DECIMAL(18, 8) NOT NULL DEFAULT 1,
    ADD COLUMN base_amount DECIMAL(15, 2);

-- An account denominated in a foreign currency still has to appear on a trial
-- balance stated in one currency, so its opening balance needs restating like
-- any other figure. The current balance is derived from ledger entries, which
-- now carry their own base amounts.
ALTER TABLE accounts
    ADD COLUMN fx_rate DECIMAL(18, 8) NOT NULL DEFAULT 1,
    ADD COLUMN base_opening_balance DECIMAL(15, 2) DEFAULT 0,
    ADD COLUMN base_current_balance DECIMAL(15, 2) DEFAULT 0;

ALTER TABLE expense_reports
    ADD COLUMN fx_rate DECIMAL(18, 8) NOT NULL DEFAULT 1,
    ADD COLUMN base_total_amount DECIMAL(15, 2) NOT NULL DEFAULT 0;

-- Expense lines have no currency of their own — they inherit the report's, so
-- they inherit its rate too and only need the restated amount.
ALTER TABLE expense_lines
    ADD COLUMN base_amount DECIMAL(15, 2);

ALTER TABLE projects
    ADD COLUMN fx_rate DECIMAL(18, 8) NOT NULL DEFAULT 1,
    ADD COLUMN base_budget DECIMAL(15, 2);

-- Payroll cost across a workforce paid in different currencies is only additive
-- once restated.
ALTER TABLE employees
    ADD COLUMN fx_rate DECIMAL(18, 8) NOT NULL DEFAULT 1,
    ADD COLUMN base_salary DECIMAL(15, 2);

-- ------------------------------------------------------------------ backfill
--
-- Every row that exists today was written while a second currency was refused,
-- so all of it is already in the base currency: the rate is 1 and the base
-- amount is the amount. This is exact, not an approximation — which is the one
-- advantage of having enforced single-currency honestly up to now instead of
-- letting mixed data accumulate.

UPDATE opportunities          SET base_value = value;
UPDATE quotes                 SET base_total = total;
UPDATE sales_orders           SET base_total = total;
UPDATE invoices               SET base_total = total,
                                  base_amount_paid = COALESCE(amount_paid, 0),
                                  base_amount_due = amount_due;
UPDATE payments               SET base_amount = amount;
UPDATE purchase_orders        SET base_total = total;
UPDATE general_ledger_entries SET base_amount = amount;
UPDATE accounts               SET base_opening_balance = COALESCE(opening_balance, 0),
                                  base_current_balance = COALESCE(current_balance, 0);
UPDATE expense_reports        SET base_total_amount = total_amount;
UPDATE expense_lines          SET base_amount = amount;
UPDATE projects               SET base_budget = budget;
UPDATE employees              SET base_salary = salary;

-- Now that they are populated, the two that are never legitimately absent can
-- say so. The rest stay nullable because the amount they restate is nullable —
-- a quote with no total has no base total either.
ALTER TABLE general_ledger_entries ALTER COLUMN base_amount SET NOT NULL;
ALTER TABLE payments             ALTER COLUMN base_amount SET NOT NULL;

-- ------------------------------------------------------- rate sanity bounds

ALTER TABLE opportunities          ADD CONSTRAINT opportunities_fx_rate_positive          CHECK (fx_rate > 0);
ALTER TABLE quotes                 ADD CONSTRAINT quotes_fx_rate_positive                 CHECK (fx_rate > 0);
ALTER TABLE sales_orders           ADD CONSTRAINT sales_orders_fx_rate_positive           CHECK (fx_rate > 0);
ALTER TABLE invoices               ADD CONSTRAINT invoices_fx_rate_positive               CHECK (fx_rate > 0);
ALTER TABLE payments               ADD CONSTRAINT payments_fx_rate_positive               CHECK (fx_rate > 0);
ALTER TABLE purchase_orders        ADD CONSTRAINT purchase_orders_fx_rate_positive        CHECK (fx_rate > 0);
ALTER TABLE general_ledger_entries ADD CONSTRAINT gl_entries_fx_rate_positive             CHECK (fx_rate > 0);
ALTER TABLE accounts               ADD CONSTRAINT accounts_fx_rate_positive               CHECK (fx_rate > 0);
ALTER TABLE expense_reports        ADD CONSTRAINT expense_reports_fx_rate_positive        CHECK (fx_rate > 0);
ALTER TABLE projects               ADD CONSTRAINT projects_fx_rate_positive               CHECK (fx_rate > 0);
ALTER TABLE employees              ADD CONSTRAINT employees_fx_rate_positive              CHECK (fx_rate > 0);

-- --------------------------------------------------- what is deliberately not
--                                                      multi-currency yet
--
-- `products.cost_price` / `sale_price` and `stock_movements.unit_cost` have no
-- currency column and do not get one here. Selling the same product in several
-- currencies is a price-list feature — one product, many prices, each with its
-- own currency and validity window — and bolting a single `currency` column
-- onto `products` would let a catalogue hold one EUR product and one USD
-- product with no way to show a customer either list coherently.
--
-- Until that exists, product prices are base-currency by definition, which is
-- what the inventory valuation report already assumes when it multiplies
-- quantity by `cost_price`. That assumption is now written down rather than
-- merely true by accident.
