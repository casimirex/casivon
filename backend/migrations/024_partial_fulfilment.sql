-- Billing an order in instalments.
--
-- An order could be invoiced exactly once, for the whole of it. Purchasing has
-- always handled the mirror case — `received_quantity` per line, an outstanding
-- figure, an over-receipt refusal and repeatable goods receipts — and sales had
-- no equivalent at all.
--
-- The cost was not merely a missing feature. A draft invoice is editable, so an
-- operator short of stock would trim the invoice to what the shelf held, issue
-- it, and end up with an order sitting terminally in `delivered` for ten units
-- with six shipped: the remaining four unbillable, because the order already had
-- its one invoice, and unrecorded, because nothing tracked how much of a line
-- had been fulfilled.

-- Which order line an invoice line is billing.
--
-- Nullable, and that is not laxness: an invoice can be raised straight against a
-- customer with no order behind it at all, and those lines have nothing to point
-- at. `ON DELETE SET NULL` for the same reason a deleted product leaves its
-- invoice line intact — the invoice is a record of what was billed, and it
-- outlives the documents it was built from.
ALTER TABLE invoice_lines
    ADD COLUMN order_line_id UUID REFERENCES sales_order_lines(id) ON DELETE SET NULL;

-- How much of an order line has been invoiced is *derived* from these rows
-- rather than counted into a column: summed across the order's invoices,
-- ignoring cancelled ones. That is what makes cancelling an invoice, deleting a
-- draft and editing a draft's lines all give the quantity back with no
-- decrementing code to write — the same reason settlement is derived from the
-- payment ledger rather than accumulated.
--
-- The index serves that sum, which runs on every conversion and every shipment.
CREATE INDEX idx_invoice_lines_order_line ON invoice_lines (order_line_id);

-- Existing conversions predate the link, so match their lines back where the
-- answer is unambiguous.
--
-- Until now an order got exactly one invoice covering it whole, and conversion
-- copied each order line verbatim — same product, same description, same
-- quantity. So a match on all three is the same line, unless one order names the
-- same product and description twice, in which case there is no way to tell
-- which invoice line came from which. Those stay NULL and read as uninvoiced,
-- which is the safe direction: the order looks like it still owes goods rather
-- than like it has been billed for something it has not.
UPDATE invoice_lines il
SET order_line_id = sol.id
FROM invoices i
JOIN sales_order_lines sol ON sol.order_id = i.order_id
WHERE il.invoice_id = i.id
  AND i.order_id IS NOT NULL
  AND il.order_line_id IS NULL
  AND sol.product_id IS NOT DISTINCT FROM il.product_id
  AND sol.description = il.description
  AND sol.quantity = il.quantity
  -- Unambiguous on both sides: exactly one order line answers to this
  -- description, and exactly one invoice line does.
  AND (
      SELECT COUNT(*) FROM sales_order_lines x
      WHERE x.order_id = i.order_id
        AND x.product_id IS NOT DISTINCT FROM il.product_id
        AND x.description = il.description
  ) = 1
  AND (
      SELECT COUNT(*) FROM invoice_lines y
      WHERE y.invoice_id = i.id
        AND y.product_id IS NOT DISTINCT FROM il.product_id
        AND y.description = il.description
  ) = 1;

-- No status is backfilled. `partially_shipped` is a state an order reaches by
-- being part-invoiced from here on; rewriting the status of orders that were
-- closed under the old rule would be inventing history.
