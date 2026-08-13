-- Automatic posting: raising and settling an invoice moves the books.
--
-- The accounting module has looked finished for a long time — chart of
-- accounts, double-entry ledger, trial balance, P&L, balance sheet — but
-- nothing has ever posted to it. Every one of those reports sums
-- `general_ledger_entries`, and the only rows in that table are ones somebody
-- typed into the manual journal form. A business that has invoiced all year has
-- an empty profit and loss.
--
-- This migration adds the two things posting needs and nothing else: somewhere
-- to record which account plays which role, and a key that makes posting the
-- same event twice impossible.

-- ------------------------------------------------------------ account mapping
--
-- Posting has to know which account is receivables and which is revenue. That
-- is configuration, and `organization_settings` is already the singleton row
-- where configuration lives — it is where `default_currency` is kept, and the
-- posting rules need both together.
--
-- All five are nullable, and that is the feature rather than a compromise:
-- **a complete mapping is what switches posting on**. An installation that has
-- not chosen its accounts keeps behaving exactly as it does today, posting
-- nothing, instead of failing the first time somebody sends an invoice. The
-- settings screen shows which of these are still empty.
--
-- ON DELETE RESTRICT: an account named here is load-bearing. Deleting it would
-- leave posting half-configured and silently off, which is the one failure mode
-- worth being loud about. `accounts` already refuses deletion once entries
-- reference it; this extends the same protection to accounts that are mapped
-- but not yet posted to.
ALTER TABLE organization_settings
    ADD COLUMN ar_account_id            UUID REFERENCES accounts(id) ON DELETE RESTRICT,
    ADD COLUMN bank_account_id          UUID REFERENCES accounts(id) ON DELETE RESTRICT,
    ADD COLUMN sales_revenue_account_id UUID REFERENCES accounts(id) ON DELETE RESTRICT,
    ADD COLUMN tax_payable_account_id   UUID REFERENCES accounts(id) ON DELETE RESTRICT,
    -- One account for both directions rather than a separate gain and loss
    -- pair: the debit/credit side already carries the sign, and the P&L shows
    -- the net, which is the figure anyone actually wants.
    ADD COLUMN fx_gain_loss_account_id  UUID REFERENCES accounts(id) ON DELETE RESTRICT;

-- The account types each role requires (asset, revenue, liability) are enforced
-- in the use case rather than here: the check needs to read the referenced row,
-- and a CHECK constraint cannot. See `SettingsUseCases::update_organization`.

-- --------------------------------------------------------------- posting key
--
-- What makes an automatic posting safe to retry.
--
-- Every entry written by the poster carries a key naming the event that caused
-- it — `sales_invoice:{uuid}:revenue`, `sales_payment:{uuid}:fx`, and the
-- `:reversal:` variants. A UNIQUE index means the second attempt at the same
-- event fails on the constraint rather than quietly doubling revenue, no matter
-- what happened above it: a retried request, two workers, a repair run racing a
-- live posting.
--
-- Manual journal entries leave this NULL. Postgres allows any number of NULLs
-- under a unique index, so the manual form is unaffected — and NULL doubles as
-- the flag for "a human wrote this", which is what decides whether an entry may
-- be deleted through the manual endpoint or has to be reversed instead.
ALTER TABLE general_ledger_entries
    ADD COLUMN posting_key TEXT;

CREATE UNIQUE INDEX idx_gl_entries_posting_key
    ON general_ledger_entries (posting_key);

-- Finding what a document posted, and finding what has not posted yet, are both
-- lookups by the document rather than by the key. `reference_type` and
-- `reference_id` have carried that since the first accounting migration and
-- already have an index; the poster fills them in on every row it writes.

-- ----------------------------------------------------------------- no backfill
--
-- Documents raised before this migration stay unposted, deliberately.
--
-- Posting a year of historical invoices would write a year of revenue into the
-- books as a side effect of running a migration, landing it in whatever period
-- the entry dates happen to fall in and changing every report the business has
-- already published. That is a decision for whoever runs the installation.
--
-- They are not lost: `GET /accounting/unposted` lists exactly these documents,
-- and `POST /accounting/post-unposted` posts them when somebody chooses to.
