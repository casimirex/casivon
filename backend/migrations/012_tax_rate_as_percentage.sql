-- One convention for tax rates: a whole percentage, everywhere.
--
-- Until now `tax_rates.rate` held a fraction (0.2000) while the document lines
-- that actually compute tax held a percentage (20.00). Both were validated, so
-- neither was broken on its own, but "tax rate" meant two different numbers in
-- one system — the kind of thing that eventually produces a 2000% tax line.
--
-- Percentage wins, for three reasons:
--   * `discount_percent` sits on the same document line and is already a
--     percentage. A fraction next to it is what makes the pair confusing.
--   * The concept users type and read is "20%". The UI already multiplied this
--     column by 100 for display, so the fraction never surfaced to anyone.
--   * Only document lines compute anything; `tax_rates` is reference data that
--     no calculation reads. Converting it touches one small table, whereas
--     converting the lines would mean rewriting the stored rate *and* the
--     computed totals on every historical document.

-- ------------------------------------------------------- tax rates: 0.2 -> 20

ALTER TABLE tax_rates ALTER COLUMN rate TYPE DECIMAL(5, 2);

-- Safe to run once: the column was constrained to 0..1, so every existing value
-- is a fraction.
UPDATE tax_rates SET rate = rate * 100;

ALTER TABLE tax_rates ADD CONSTRAINT tax_rates_rate_is_a_percentage
    CHECK (rate >= 0 AND rate <= 100);

-- ------------------------------------- purchase order lines: the missing rate

-- `PurchaseOrderLineRequest` has always accepted `tax_rate` and the PO total is
-- computed from it, but there was nowhere to put it: the line was written
-- without the rate, so it could not be shown, edited or recomputed afterwards.
ALTER TABLE purchase_order_lines
    ADD COLUMN tax_rate DECIMAL(5, 2) NOT NULL DEFAULT 0;

-- Recover the rate for existing lines from their order's own totals. Exact when
-- every line on the order shares one rate, which is the ordinary case; where
-- they differ this spreads the order's average across its lines. Either way the
-- order's stored total stays reconcilable, which `DEFAULT 0` would not.
UPDATE purchase_order_lines line
SET tax_rate = ROUND(po.tax_amount / po.subtotal * 100, 2)
FROM purchase_orders po
WHERE po.id = line.po_id
  AND po.subtotal IS NOT NULL
  AND po.subtotal <> 0
  AND po.tax_amount IS NOT NULL;

ALTER TABLE purchase_order_lines ADD CONSTRAINT purchase_order_lines_tax_rate_range
    CHECK (tax_rate >= 0 AND tax_rate <= 100);

-- ------------------------------------------- upper bounds the lines were given
--
-- `009` capped `discount_percent` on the tables it created but left `tax_rate`
-- with only a lower bound, and `quote_lines` predates both.

ALTER TABLE quote_lines ADD CONSTRAINT quote_lines_tax_rate_range
    CHECK (tax_rate >= 0 AND tax_rate <= 100);
ALTER TABLE quote_lines ADD CONSTRAINT quote_lines_discount_range
    CHECK (discount_percent >= 0 AND discount_percent <= 100);

ALTER TABLE sales_order_lines ADD CONSTRAINT sales_order_lines_tax_rate_max
    CHECK (tax_rate <= 100);
ALTER TABLE invoice_lines ADD CONSTRAINT invoice_lines_tax_rate_max
    CHECK (tax_rate <= 100);

ALTER TABLE products ADD CONSTRAINT products_tax_rate_range
    CHECK (tax_rate IS NULL OR (tax_rate >= 0 AND tax_rate <= 100));
