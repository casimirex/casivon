-- Perpetual inventory: stock becomes an asset on the balance sheet, and its
-- cost reaches the profit and loss when the goods leave rather than when they
-- arrive.
--
-- Until now `receipt_entries` debited **Cost of sales** the moment goods were
-- received. Buy £10,000 of stock and sell none of it, and the P&L shows a
-- £10,000 expense while the balance sheet shows nothing: the books say the money
-- was spent and there is nothing to show for it. Every other posting path is
-- sound; this was the one that left a balance sheet materially wrong for anybody
-- holding stock.
--
-- Costing is a **moving weighted average**, and the cost of a sale is posted
-- when the outward stock movement is recorded — which is how stock has always
-- left, since there is no automatic stock-out anywhere in the application.

-- ------------------------------------------------------------- what a unit costs
--
-- Four decimal places rather than the usual two. This is a cost *per unit*, and
-- a genuine average lands on figures like 3.3333 all the time; rounding that to
-- pennies per unit drifts badly over a few thousand of them. Amounts posted to
-- the ledger are still rounded to two — the extra precision lives here, not in
-- the journal.
ALTER TABLE products
    ADD COLUMN average_cost DECIMAL(15, 4);

-- The only cost information that exists today. `cost_price` is a standing figure
-- somebody typed in rather than anything derived from what was actually paid, so
-- this is a starting point and not a restatement: the first receipt after this
-- migration begins moving the average toward what the goods really cost.
UPDATE products SET average_cost = cost_price WHERE cost_price IS NOT NULL;

-- -------------------------------------------------------- what a movement cost
--
-- `stock_movements.unit_cost` already exists, but it is the purchase price **in
-- the order's currency**: the goods receipt copies `po_line.unit_price` straight
-- in, while the ledger restates the same figure at the order's rate. So a EUR
-- purchase leaves a EUR number in a column nothing restates, and valuing stock
-- from it would mix currencies silently.
--
-- Valuation therefore gets its own column, in the base currency, stored rather
-- than derived — the same decision every other `base_*` column has made since
-- `013_multi_currency.sql`. `unit_cost` keeps the meaning it already had.
ALTER TABLE stock_movements
    ADD COLUMN base_unit_cost DECIMAL(15, 4);

-- Imperfect on purpose, and worth being explicit about: for historical movements
-- in a foreign currency this copies a figure that was never restated. It is the
-- only number that exists, the alternative is NULL, and no posting is being made
-- from it — these rows are already in the books at whatever the receipt posted.
UPDATE stock_movements SET base_unit_cost = unit_cost WHERE unit_cost IS NOT NULL;

-- ------------------------------------------------------------ two more roles
--
-- Nullable, like the ten before them, and for the same reason: a complete
-- mapping is what switches a posting rule on. Leaving these empty keeps an
-- existing installation behaving *exactly* as it does today — receipts still
-- debit Cost of sales, and nothing anywhere starts failing.
--
-- They are deliberately not added to the existing `AccountMapping`, which is
-- all-or-nothing by design. Making them required there would stop every posting
-- on every existing install until an admin mapped two new accounts, which is a
-- far worse failure than the one this change exists to fix. They get their own
-- mapping instead, so inventory posting switches on independently.
ALTER TABLE organization_settings
    -- The asset. Stock sits here between arriving and being sold.
    ADD COLUMN inventory_account_id            UUID REFERENCES accounts(id) ON DELETE RESTRICT,
    -- Where hand-made movements find their other leg. Without it, an adjustment
    -- typed in by a stock-taker would change the valuation report and not the
    -- ledger, and the two would drift apart with nothing to show why.
    ADD COLUMN inventory_adjustment_account_id UUID REFERENCES accounts(id) ON DELETE RESTRICT;

-- No retro-posting, the same choice `014_gl_posting.sql` made: posted facts are
-- permanent, and rewriting a year of receipts from Cost of sales to Inventory
-- would restate closed periods.
--
-- That leaves a real discontinuity — stock already on hand was expensed on
-- arrival, so selling it now would credit an Inventory account that was never
-- debited. `POST /accounting/inventory-opening` is the one-time operator action
-- that squares it, and it shows what it will post before it posts anything.
