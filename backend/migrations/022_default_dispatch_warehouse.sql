-- Where goods ship from when an invoice is issued.
--
-- Selling has never moved stock. Receiving goods creates a movement
-- automatically; issuing an invoice did not, so the only way stock left the
-- shelf was somebody remembering to record a movement by hand.
--
-- The last two changes made that conspicuous rather than less so: a purchase
-- return takes stock off automatically, and a sales credit note puts stock
-- *back* automatically when a warehouse is named. The application was reversing
-- a movement it never made.
--
-- This does not change where the cost of a sale posts. That still happens when
-- the stock movement is recorded — the movement simply now happens at issue
-- rather than by hand, so revenue and its cost land together.

ALTER TABLE organization_settings
    -- Nullable, and unset means **nothing ships automatically** — an existing
    -- installation keeps issuing invoices without moving stock until somebody
    -- chooses a warehouse. Every opt-in in this schema works this way: the
    -- posting mapping, the inventory pair, the object store endpoint.
    --
    -- It matters more than usual here. Once this is set, issuing an invoice for
    -- stock that is not on the shelf is *refused*, because that is what
    -- `record_movement` does and this reuses it. Catching that at the moment the
    -- invoice is raised is the right answer, but it is a behaviour change with
    -- teeth, and opting in is how somebody consents to it.
    --
    -- ON DELETE RESTRICT: a warehouse named here is load-bearing. Deleting it
    -- would leave invoicing pointing at nothing and silently stop shipping,
    -- which is exactly the failure mode worth being loud about.
    ADD COLUMN default_dispatch_warehouse_id UUID REFERENCES warehouses(id) ON DELETE RESTRICT;

-- No backfill, and deliberately not "pick the only warehouse if there is one".
-- Guessing here would switch on a behaviour change nobody asked for, on the
-- first upgrade, with no warning.
