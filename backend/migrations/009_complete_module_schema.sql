-- Fills the gaps the first eight migrations left behind:
--   * sales orders and invoices had no line-item tables
--   * stock transfers had nowhere to record the destination warehouse
--   * goods receipts had no warehouse to receive into
--   * leave approval had no entitlement to check against
--   * document numbers had no race-free source

-- ---------------------------------------------------------------- sales lines

CREATE TABLE sales_order_lines (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    order_id UUID NOT NULL REFERENCES sales_orders(id) ON DELETE CASCADE,
    product_id UUID REFERENCES products(id) ON DELETE SET NULL,
    description TEXT NOT NULL,
    quantity INTEGER NOT NULL DEFAULT 1 CHECK (quantity > 0),
    unit_price DECIMAL(15, 2) NOT NULL,
    discount_percent DECIMAL(5, 2) NOT NULL DEFAULT 0 CHECK (discount_percent >= 0 AND discount_percent <= 100),
    tax_rate DECIMAL(5, 2) NOT NULL DEFAULT 0 CHECK (tax_rate >= 0),
    line_total DECIMAL(15, 2) NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE invoice_lines (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    product_id UUID REFERENCES products(id) ON DELETE SET NULL,
    description TEXT NOT NULL,
    quantity INTEGER NOT NULL DEFAULT 1 CHECK (quantity > 0),
    unit_price DECIMAL(15, 2) NOT NULL,
    discount_percent DECIMAL(5, 2) NOT NULL DEFAULT 0 CHECK (discount_percent >= 0 AND discount_percent <= 100),
    tax_rate DECIMAL(5, 2) NOT NULL DEFAULT 0 CHECK (tax_rate >= 0),
    line_total DECIMAL(15, 2) NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_sales_order_lines_order ON sales_order_lines(order_id);
CREATE INDEX idx_invoice_lines_invoice ON invoice_lines(invoice_id);
CREATE INDEX idx_quote_lines_quote ON quote_lines(quote_id);

-- quote_lines predates the NOT NULL/CHECK conventions used above; align it.
ALTER TABLE quote_lines
    ALTER COLUMN discount_percent SET DEFAULT 0,
    ALTER COLUMN tax_rate SET DEFAULT 0;
UPDATE quote_lines SET discount_percent = 0 WHERE discount_percent IS NULL;
UPDATE quote_lines SET tax_rate = 0 WHERE tax_rate IS NULL;
ALTER TABLE quote_lines
    ALTER COLUMN discount_percent SET NOT NULL,
    ALTER COLUMN tax_rate SET NOT NULL;

-- Sales orders may be raised straight from a quote; record the link both ways.
CREATE INDEX idx_sales_orders_quote ON sales_orders(quote_id);
CREATE INDEX idx_invoices_order ON invoices(order_id);

-- ------------------------------------------------------------- stock movement

-- A transfer moves stock between two warehouses: warehouse_id is the source,
-- to_warehouse_id the destination. NULL for every other movement type.
ALTER TABLE stock_movements
    ADD COLUMN to_warehouse_id UUID REFERENCES warehouses(id) ON DELETE RESTRICT;

ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_transfer_has_destination
    CHECK (movement_type <> 'transfer' OR to_warehouse_id IS NOT NULL);

CREATE INDEX idx_stock_movements_warehouse ON stock_movements(warehouse_id);
CREATE INDEX idx_stock_movements_reference ON stock_movements(reference_type, reference_id);
CREATE INDEX idx_stock_levels_warehouse ON stock_levels(warehouse_id);
CREATE INDEX idx_bom_lines_bom ON bom_lines(bom_id);

-- ---------------------------------------------------------------- purchasing

-- Receiving goods has to land them somewhere.
ALTER TABLE goods_receipts
    ADD COLUMN warehouse_id UUID REFERENCES warehouses(id) ON DELETE RESTRICT;

CREATE INDEX idx_po_lines_po ON purchase_order_lines(po_id);
CREATE INDEX idx_goods_receipts_po ON goods_receipts(po_id);
CREATE INDEX idx_goods_receipt_lines_receipt ON goods_receipt_lines(receipt_id);

ALTER TABLE purchase_order_lines
    ADD CONSTRAINT po_lines_received_not_negative CHECK (received_quantity >= 0);

-- ------------------------------------------------------------------------ hr

ALTER TABLE employees
    ADD COLUMN annual_leave_entitlement INTEGER NOT NULL DEFAULT 25
    CHECK (annual_leave_entitlement >= 0);

ALTER TABLE leave_requests
    ADD CONSTRAINT leave_requests_dates_ordered CHECK (end_date >= start_date);

ALTER TABLE leave_requests
    ADD CONSTRAINT leave_requests_days_positive CHECK (days_requested > 0);

CREATE INDEX idx_expense_lines_report ON expense_lines(expense_report_id);
CREATE INDEX idx_employees_user ON employees(user_id);

-- ------------------------------------------------------------------ projects

CREATE INDEX idx_tasks_parent ON tasks(parent_task_id);
CREATE INDEX idx_tasks_status ON tasks(status);
CREATE INDEX idx_time_entries_employee ON time_entries(employee_id);
CREATE INDEX idx_time_entries_date ON time_entries(entry_date);

-- ---------------------------------------------------------- document numbers

-- Sequences rather than SELECT MAX(...) + 1: concurrent creates would otherwise
-- race and collide on the UNIQUE document-number constraints.
CREATE SEQUENCE quote_number_seq;
CREATE SEQUENCE sales_order_number_seq;
CREATE SEQUENCE invoice_number_seq;
CREATE SEQUENCE purchase_order_number_seq;
CREATE SEQUENCE goods_receipt_number_seq;
CREATE SEQUENCE expense_report_number_seq;
CREATE SEQUENCE employee_number_seq;
CREATE SEQUENCE project_code_seq;

-- Returns e.g. next_document_number('QUO') -> 'QUO-2026-000042'
CREATE OR REPLACE FUNCTION next_document_number(prefix TEXT, seq_name TEXT)
RETURNS TEXT AS $$
DECLARE
    next_val BIGINT;
BEGIN
    EXECUTE format('SELECT nextval(%L)', seq_name) INTO next_val;
    RETURN prefix || '-' || to_char(NOW(), 'YYYY') || '-' || lpad(next_val::TEXT, 6, '0');
END;
$$ LANGUAGE plpgsql;

-- ------------------------------------------------------------------ accounts

CREATE INDEX idx_accounts_parent ON accounts(parent_id);
CREATE INDEX idx_gl_entries_debit_account ON general_ledger_entries(debit_account_id);
CREATE INDEX idx_gl_entries_credit_account ON general_ledger_entries(credit_account_id);
CREATE INDEX idx_bank_accounts_account ON bank_accounts(account_id);

-- Double-entry sanity: an entry may never post to the same account twice, and
-- amounts are always positive (direction is expressed by the two account columns).
ALTER TABLE general_ledger_entries
    ADD CONSTRAINT gl_entries_distinct_accounts CHECK (debit_account_id <> credit_account_id);
ALTER TABLE general_ledger_entries
    ADD CONSTRAINT gl_entries_amount_positive CHECK (amount > 0);
