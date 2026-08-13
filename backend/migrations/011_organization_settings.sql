-- Organisation profile: the company details that belong on a quote or invoice.
--
-- Deliberately a single row, not a tenant table. `users.org_id` and the `org_id`
-- columns on the document tables are nullable and nothing sets them; multi-
-- tenancy is not implemented, and a table that looked like it supported tenants
-- would imply isolation the rest of the schema does not enforce.
--
-- The `singleton` column is the whole mechanism: a CHECK pins it to true and a
-- UNIQUE constraint means only one row can carry that value.

CREATE TABLE organization_settings (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    singleton BOOLEAN NOT NULL DEFAULT true UNIQUE CHECK (singleton),
    name VARCHAR(200) NOT NULL,
    legal_name VARCHAR(200),
    email VARCHAR(255),
    phone VARCHAR(50),
    website VARCHAR(255),
    tax_number VARCHAR(50),
    address_line1 VARCHAR(200),
    address_line2 VARCHAR(200),
    city VARCHAR(100),
    postal_code VARCHAR(20),
    country VARCHAR(100),
    -- Default currency for new documents. Three-letter ISO code, matching the
    -- `currency` columns already on quotes, orders and invoices.
    default_currency CHAR(3) NOT NULL DEFAULT 'USD',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TRIGGER update_organization_settings_updated_at BEFORE UPDATE ON organization_settings
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Seed the row so the settings screen always has something to edit. The name is
-- a placeholder the first admin is expected to replace.
INSERT INTO organization_settings (name) VALUES ('My Company');
