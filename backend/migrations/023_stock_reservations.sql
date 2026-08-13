-- Holding stock for a confirmed order.
--
-- Issuing an invoice now takes goods off the shelf, so a confirmed order that
-- reserves nothing is a promise with nothing behind it: two orders can both be
-- confirmed against the last unit, and whichever is invoiced second is refused —
-- in front of a customer who was already told they could have it.
--
-- `stock_levels.reserved_quantity` has been in the schema since the inventory
-- module was written. `available()` subtracts it, the low-stock query subtracts
-- it, and `record_movement` — the single door stock changes through — checks
-- against it. Nothing has ever written it. The column, the accessor and the
-- consumer were all in place; only the writer was missing.

CREATE TABLE stock_reservations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

    order_id UUID NOT NULL REFERENCES sales_orders(id) ON DELETE CASCADE,

    -- One reservation per line at a time. Editing a confirmed order releases and
    -- re-reserves rather than accumulating, and this constraint is what makes
    -- that impossible to get wrong by accident.
    order_line_id UUID NOT NULL UNIQUE REFERENCES sales_order_lines(id) ON DELETE CASCADE,

    product_id UUID NOT NULL REFERENCES products(id),
    warehouse_id UUID NOT NULL REFERENCES warehouses(id),

    -- What was actually held, which is **not** the ordered quantity.
    --
    -- Confirming an order short of stock still confirms, reserving what is on
    -- the shelf and leaving the rest unreserved — selling before buying is
    -- ordinary, and refusing would block it outright. So order 10 against 6 on
    -- hand reserves 6, and releasing later has to give back exactly the 6 that
    -- was taken. That is the whole reason this is a table rather than a figure
    -- derived from the order line when needed.
    quantity INTEGER NOT NULL CHECK (quantity > 0),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_stock_reservations_order ON stock_reservations (order_id);
CREATE INDEX idx_stock_reservations_stock ON stock_reservations (product_id, warehouse_id);

-- No column is added to `stock_levels`: `reserved_quantity` already exists and
-- becomes the running total these rows sum to, moved in the same transaction
-- that writes them.
--
-- Nothing is backfilled. Orders confirmed before this existed hold no
-- reservation, and inventing one would take stock away from shelves that have
-- been counted without it — re-confirming or editing an order is how an
-- operator opts a specific order in.
