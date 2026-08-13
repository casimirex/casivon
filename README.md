# Casivon — Rust + PostgreSQL + Vite/React

A production-ready ERP built with clean architecture principles.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        FRONTEND                              │
│              Vite + React + TypeScript + Tailwind            │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐           │
│  │  Auth   │ │  CRM    │ │  Sales  │ │Inventory│ ...       │
│  └─────────┘ └─────────┘ └─────────┘ └─────────┘           │
└────────────────────────┬────────────────────────────────────┘
                         │ HTTP/REST
┌────────────────────────┴────────────────────────────────────┐
│                        BACKEND (Rust/Axum)                   │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  API Layer    →  Routes, Middleware, Validation        │ │
│  ├────────────────────────────────────────────────────────┤ │
│  │  App Layer    →  Use Cases, DTOs, Orchestration        │ │
│  ├────────────────────────────────────────────────────────┤ │
│  │  Domain Layer →  Entities, Traits, Business Rules      │ │
│  ├────────────────────────────────────────────────────────┤ │
│  │  Infra Layer  →  Repositories, DB, Cache, External API │ │
│  └────────────────────────────────────────────────────────┘ │
└────────────────────────┬────────────────────────────────────┘
                         │ SQLx
                    ┌────┴────┐
                    │PostgreSQL│
                    └─────────┘
```

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Backend | Rust, Axum, SQLx, Tokio |
| Database | PostgreSQL 16 |
| Cache | Redis |
| Frontend | Vite, React 18, TypeScript, Tailwind CSS, shadcn/ui |
| Auth | JWT (access + refresh tokens) |
| Validation | validator crate (backend), Zod (frontend) |

## Modules

1. **Auth** — Users, roles, permissions, JWT sessions
2. **CRM** — Contacts, companies, leads, opportunities, activities
3. **Sales** — Quotes, orders, invoices, payments
4. **Inventory** — Products, SKUs, warehouses, stock movements, BOMs
5. **Purchasing** — Vendors, purchase orders, goods receipts
6. **Accounting** — Chart of accounts, GL entries, bank reconciliation
7. **HR** — Employees, leave requests, expense reports
8. **Projects** — Projects, tasks, time tracking, Gantt

## Quick Start

```bash
make setup      # copies backend/.env from the template, installs frontend deps
make up         # Postgres and Redis  (make up-all adds Mailpit and MinIO)
make backend    # the API on :8080 — applies its migrations on boot
make frontend   # in another terminal: the web app on :3000
```

`make` on its own lists every target, and nothing is hidden — each recipe is a
command you could have typed yourself. `make -n <target>` prints one without
running it. The equivalent by hand:

```bash
docker compose up -d postgres redis minio
cd backend  && cp .env.example .env && cargo run
cd frontend && npm install && npm run dev
```

Open http://localhost:3000 and create an account. **The first account registered
becomes the administrator** — without that bootstrap nobody could reach the
role-gated modules, since granting a role itself requires an admin.

Colleagues then register themselves and arrive as `user`. An admin grants them a
role under **Settings → Users** (`admin`, `manager`, `accountant`, `hr`, `sales`,
`user`), and fills in the company details under **Settings → Company**, which is
what identifies the business on quotes, invoices and purchase orders.

Accounts are retired rather than deleted: every document keeps its author, and a
retired account can be restored. An admin cannot demote or deactivate
themselves — locking the last administrator out is not recoverable from inside
the application.

## Tests

```bash
make test           # 166 unit + 281 integration (backend), 150 (frontend)
make backend-test   # or one side at a time
make frontend-test
make gate           # everything CI would run: both suites, tsc, eslint, both builds
```

The backend integration tests in `backend/tests/` need Postgres running
(`make up`) and `DATABASE_URL` set — `backend/.env`
is picked up automatically. `tests/redis_revocation.rs` additionally needs
Redis; everything else runs the token denylist in memory so the suite stays
hermetic. Each test gets its own throwaway database from
`#[sqlx::test]` with the migrations applied, then mounts the real router over it
and drives it in-process, so tests share no state and can run in any order. The
databases are named `_sqlx_test_*` and are cleaned up on the following run.

## Make targets

`make` lists them all. The ones worth knowing:

| | |
|---|---|
| `make setup` | first run: `backend/.env` from the template, frontend dependencies |
| `make up` / `up-all` | Postgres and Redis / plus Mailpit and MinIO |
| `make down` / `restart` / `ps` / `logs` | the stack, keeping data (`make logs SERVICE=postgres` for one) |
| `make backend` / `frontend` | run each side |
| `make test` / `gate` | both suites / everything CI would run |
| `make generate` | regenerate `openapi.json` and the frontend's types from the handlers |
| `make db-shell` | `psql` into the running database |
| `make db-backup` | dump to `~/casivon-backups`, outside the repo so it cannot be committed |
| `make db-restore FILE=…` | load a dump back in |
| `make db-reset` | drop and recreate the database, then let the backend migrate it |
| `make clean` | stop the stack **and delete its volumes** |

The three that destroy data — `clean`, `db-reset`, `db-restore` — ask for a typed
`yes` first. `make db-backup` before any of them; the dumps are timestamped and
gzipped beside the plain file.

## Development Guide

### Adding a New Module

1. **Backend**: Create folder in `src/modules/{module_name}/`
   - `domain/` — entities, traits, errors
   - `application/` — use cases, DTOs
   - `infrastructure/` — repository impl, routes
   - Register in `src/modules/mod.rs` and `src/app.rs`

2. **Frontend**: Create folder in `src/modules/{ModuleName}/`
   - `api/` — API client functions
   - `components/` — UI components
   - `pages/` — route pages
   - `types/` — TypeScript interfaces
   - Register routes in `src/App.tsx`

### Database Migrations

```bash
cd backend
cargo sqlx migrate add create_{table_name}
# Edit the generated .sql file
cargo sqlx migrate run
```

## API Reference

The API documents itself. With the backend running:

| | |
|---|---|
| Swagger UI | http://localhost:8080/api/docs |
| OpenAPI 3.1 document | http://localhost:8080/api/v1/openapi.json |

156 operations across 10 tags, generated from the handlers by
[utoipa](https://github.com/juhaku/utoipa) — the annotations sit next to the
code they describe, so the document cannot describe an endpoint that no longer
exists. Two tests in `tests/openapi.rs` enforce the other direction: one parses
the route tables and fails if a route carries no annotation, the other probes
every documented operation against the real router. A path listed in the
aggregator but missing its annotation is a compile error.

Both pages are public. An API reference nobody can read without a token is not
much of a reference; the endpoints it describes are still protected. Use
**Authorize** in Swagger UI to paste an access token from `/auth/login`.

### Frontend types are generated from it

`frontend/src/types/index.ts` no longer describes the wire by hand. Each entity
type aliases a schema in `src/api/schema.d.ts`, generated from this document, so
changing a Rust DTO and forgetting the frontend is a type error rather than a
runtime surprise.

```bash
cd frontend
npm run generate:spec    # cargo run --bin openapi > openapi.json  (needs cargo)
npm run generate:types   # openapi.json -> src/api/schema.d.ts
```

Both `openapi.json` and `schema.d.ts` are committed so a clone builds without a
Rust toolchain, which means both can go stale — so both links are tested. A
backend test compares the committed `openapi.json` against the live document; a
frontend test regenerates `schema.d.ts` and diffs it. Either failing tells you
exactly which command to run.

What stays hand-written is what the document cannot express: the semantic string
aliases (`Money`, `IsoDate`), the *generic* response envelope — the document can
only name one concrete wrapper per payload — and the constant arrays, which are
runtime values rather than types.

## API Conventions

- Base path: `/api/v1`
- Resources: plural nouns (`/api/v1/contacts`)
- Actions: POST for create, GET for read, PUT for update, DELETE for remove
- Pagination: `?page=1&per_page=20` (`per_page` is clamped to 200)
- Filtering: `?status=active&search=john`
- Sorting: `?sort=-created_at` (minus = descending). Each endpoint has an
  allow-list of sortable columns; anything else falls back to the default, since
  a column name cannot be a bound parameter.

### Response shape

Every response carries the same envelope:

```jsonc
// single resource
{ "success": true, "data": { ... } }

// list
{ "success": true, "data": [ ... ], "pagination": { "page": 1, "per_page": 20, "total": 100, "total_pages": 5 } }

// error
{ "success": false, "error": { "code": 422, "message": "email: Invalid email format" } }
```

### Sessions

Login issues a short-lived access token (15 min) and a refresh token (7 days).
`POST /api/v1/auth/logout` revokes the refresh token by recording its `jti` in
Redis until the moment it would have expired anyway, so the denylist stays
proportional to sign-outs in flight rather than to users. `/auth/refresh` checks
that list before issuing a new pair.

Sign-out takes the **refresh** token and needs no `Authorization` header — an
access token lasts fifteen minutes, and refusing to sign out the moment one
expires would strand exactly the sessions most in need of ending. Holding the
refresh token is itself the authority to revoke it.

### Password reset

`POST /auth/forgot-password` emails a one-hour, single-use link;
`POST /auth/reset-password` spends it. Both are public — someone who cannot sign
in is exactly who needs them.

- **No account enumeration.** The request endpoint answers identically for a
  registered and an unregistered address, including when delivery fails, so it
  cannot be used to test whether an address has an account here.
- **Tokens are stored hashed** (SHA-256 of 256 random bits). A leaked copy of
  `password_reset_tokens` is not enough to take over an account. A fast hash is
  right here — slow hashing exists to make guessing a *password* expensive and
  buys nothing against that much randomness.
- **Resetting ends every session.** Each user carries a `session_epoch` stamped
  into their tokens; changing the password bumps it, and `/auth/refresh` rejects
  tokens carrying the old value. A counter rather than a cut-off timestamp,
  because a JWT `iat` claim has only second resolution and could not separate a
  token issued just before a reset from one issued just after.
- **One email per minute per account**, so a public endpoint cannot be used to
  bury someone in mail.

### Mail

Set `SMTP_HOST` and mail is delivered through that relay; leave it unset and
every message is written to the log instead, with a warning at start-up saying
so. The choice is made once, at boot, so a misconfigured relay fails while
someone is watching rather than on somebody's first password reset.

| Variable | |
|---|---|
| `SMTP_HOST` | Setting it is what enables real delivery |
| `SMTP_PORT` | Defaults to 587 / 465 / 25 depending on encryption |
| `SMTP_ENCRYPTION` | `starttls` (default), `tls`, or `none` |
| `SMTP_FROM` | Required once `SMTP_HOST` is set — `Name <you@example.com>` |
| `SMTP_USERNAME` / `SMTP_PASSWORD` | Optional; omit for a relay that authenticates by IP |

For development, `docker compose up -d mailpit` starts a catcher on port 1025
with a web UI on http://localhost:8025, so the whole reset flow can be clicked
through without sending anything to a real address:

```bash
SMTP_HOST=127.0.0.1 SMTP_PORT=1025 SMTP_ENCRYPTION=none \
  SMTP_FROM="ERP System <no-reply@erp.local>" cargo run
```

`tests/smtp_delivery.rs` sends through that catcher and reads the message back,
so delivery is tested rather than assumed. It skips when Mailpit is not running.

`APP_BASE_URL` is what the link in the email is built from.

Two consequences of the session model worth knowing:

- The access token issued alongside it keeps working until it expires (≤ 15
  minutes). Revoking access tokens too would put a denylist lookup on every
  authenticated request; the short lifetime is what bounds the window instead.
- The denylist check fails closed. If Redis is unreachable, `/auth/refresh`
  rejects rather than waving tokens through, so sessions end within 15 minutes
  of a Redis outage.

### Currency

Documents are raised in whatever currency the customer agreed, and every amount
is **also stored restated in the organisation's base currency** (`Settings →
Company`). Reports add the restated column, so a euro sale and a dollar sale can
appear in one total without either being altered.

Three things travel on every money-bearing document:

- `currency` — what the customer sees, never converted;
- `fx_rate` — the rate in force **on the document's own date**, frozen onto the
  row when it was raised. Stored rather than looked up again, so re-opening a
  closed invoice does not restate it at today's rate;
- `base_*` — the amount in the base currency, computed once and kept. A posted
  base amount is a fact, so correcting a rate later does not retroactively move
  revenue that was already reported.

Rates live in `fx_rates` and are managed under `Settings → Exchange rates`. They
are **effective-dated**: a rate stays in force until a later one supersedes it,
and one entered today does not reach back and rewrite last quarter. `rate` is
units of base per one unit of the foreign currency, so restating is always a
multiplication.

The base currency itself never gets a rate row — it is worth 1 of itself by
definition, and a stored, editable `1` is a number somebody eventually sets to
`0.98`. A currency with **no rate on the document's date is refused**, rather
than defaulted to parity: booking a EUR 10,000 invoice as USD 10,000 of revenue
is the kind of error nothing downstream would ever flag.

**Realised FX gain and loss.** A payment is made in the invoice's currency but
at the rate on the day the money arrived. An invoice for EUR 1,000 raised at
1.10 and settled at 1.15 brings in USD 50 more than the revenue booked, and that
difference is stored on the payment as `fx_gain_loss`. The invoice's own paid
and due figures stay restated at the *invoice's* rate, so they always reconcile
against its base total — the difference belongs on the payment rather than
smeared into the receivable.

The organisation's base currency still **cannot be changed once anything has
been raised**: every rate on file is quoted against it, and every stored base
amount was computed with one.

Not yet multi-currency: `products` has no currency column, so product prices are
base-currency by definition (which is what the inventory valuation report
already assumes). Selling one product at prices in several currencies is a
price-list feature. Unrealised revaluation of open balances at period end needs
a period-close concept that does not exist yet, and posting FX differences to
the ledger needs automatic GL posting from sales documents, which is also not
built — the ledger is entirely manual today.

### Automatic posting

Issuing an invoice books the revenue and the receivable; recording a payment
clears it. Before this existed the ledger only ever held what somebody typed
into the journal form, so a business could invoice all year and its profit and
loss would read zero.

**Posting is switched on by configuring it.** Five roles have to be mapped to
accounts under `Settings → Automatic posting`:

| Role | Account type |
|---|---|
| Accounts receivable | asset |
| Bank | asset |
| Sales revenue | revenue |
| Tax payable | liability |
| Foreign exchange gain/loss | revenue |

Until all five are chosen nothing posts at all, and the settings screen names
what is missing. An installation that never configures it behaves exactly as it
did before — which is what makes this safe to upgrade into.

Each account is checked when it is mapped: right type, active, and denominated
in the base currency. Every automatic entry is made in the base currency from
the stored `base_*` amounts, and a journal entry has to agree with the accounts
it touches, so a foreign-currency account here would fail at the moment an
invoice was sent rather than when it was chosen.

**What posts.** `general_ledger_entries` holds one debit and one credit, so a
three-sided posting becomes two entries:

- *Invoice issued* — receivable debited with the total; revenue credited with
  the total less tax; tax payable credited with the tax. The revenue figure is
  derived as the remainder rather than restated on its own, because rounding
  subtotal and tax separately can land a cent away from the restated total and
  post an invoice whose legs do not add up to the receivable it created.
- *Payment received* — bank debited with what the money was worth, receivable
  credited. When the rate has moved since the invoice, a second entry moves the
  difference to the FX account so the receivable still clears at the rate it was
  raised at.
- *Cancelled or reversed* — the **mirror is posted**, dated the day it happens.
  Nothing is deleted, and cancelling in April does not reach back and change
  March. Entries a document posted cannot be deleted by hand at all.

**Safe to retry.** Every automatic entry carries a unique `posting_key` naming
the event that caused it, so a second attempt is refused by the database rather
than doubling revenue. Posting runs in its own transaction after the document
write, so a crash between the two can leave an invoice issued but unposted —
visible under `GET /accounting/unposted` and fixable with
`POST /accounting/post-unposted`, which is safe to run repeatedly. Documents
raised before the feature existed appear there too; the migration deliberately
backfills nothing, because posting a year of history is a decision for whoever
runs the installation rather than a side effect of an upgrade.

**The spending side.** Receiving goods debits cost of sales and credits accounts
payable; paying the supplier clears it. Approving an expense claim debits the
expense and credits what you owe the employee; reimbursing settles it. Both
payment kinds handle a moved exchange rate the same way — a gain debits the
control account and credits the FX account, whether that control is receivables
or payables, because the leg exists to pull it back to what the document booked.

Purchase tax is split out rather than folded into cost, because input tax is
usually recoverable. Where the *cost* of goods lands depends on the costing
method — see **Stock costing** below.

**Ten roles, all or nothing.** Adding the purchase and expense roles took the
mapping from five to ten, and posting stays all-or-nothing on a complete
mapping. An installation that had configured the five sales roles will find
posting **switched off** after upgrading until it chooses the other five — the
settings screen names exactly which. A partial mapping would post lopsided
entries, which is worse than posting nothing.

The two stock roles are the exception, and deliberately so: see below.

Not posted yet: vendor credit notes, purchase returns and payroll.

### Stock costing

Receiving goods used to debit **cost of sales** the day they arrived. Buy
£10,000 of stock and sell none of it, and the P&L showed a £10,000 expense while
the balance sheet showed nothing — the books said the money was spent and there
was nothing to show for it. Every other posting path was sound; this was the one
that left a balance sheet materially wrong for anybody holding stock.

Mapping two more roles switches costing from **periodic** to **perpetual**:

| Role | Account type |
|---|---|
| Inventory | asset |
| Inventory adjustment | expense |

**These two are optional, and that is load-bearing.** Every other role is
all-or-nothing: a partial mapping posts nothing. Making these required would
have switched off sales, purchase and expense posting on every existing
installation until an admin went and mapped them — a far worse failure than the
costing it replaces. They get their own mapping (`InventoryMapping`), so
inventory posting turns on independently and leaving them empty changes nothing.

**Costing is a moving weighted average.** One running cost per product, nudged
toward the purchase price every time more arrives:

```
100 @ 4.00, then 50 @ 5.50  ->  (400 + 275) / 150 = 4.50
sell 60                     ->  cost of sale 270.00, average still 4.50
```

Kept to four decimal places rather than two, because a real average lands on
3.3333 constantly and rounding that *per unit* drifts badly over a few thousand
of them. Journal amounts are still rounded to cents, once, where they become
journal amounts. Only arrivals move the average; a sale consumes at whatever it
is at that moment, and the figure used is written onto the movement so a later
delivery cannot retrospectively change what an earlier sale cost.

**The cost of a sale is posted when the stock movement is recorded**, not when
the invoice is issued. There is no automatic stock-out anywhere in the
application — stock has only ever left through `POST /inventory/movements` — so
hanging the cost there means the sales flow is untouched and no warehouse has to
be guessed at. The trade-off is that revenue and its cost can land in different
periods if goods are issued in a different month from the invoice.

What a movement posts depends on why the stock moved:

| Movement | Caused by | Posts |
|---|---|---|
| `in` | a goods receipt | nothing — the receipt already debited Inventory |
| `in` | somebody by hand | Dr Inventory / Cr Inventory adjustment |
| `out` | anything | Dr Cost of sales / Cr Inventory |
| `transfer` | — | nothing; the value did not change, only its shelf |
| `adjustment` | — | Inventory against the adjustment account, by sign |

A shortfall is an *adjustment* rather than a cost of sale on purpose: both are
stock leaving, but only one of them earned revenue, and burying shrinkage in cost
of sales hides it from the person whose job it is to notice. The adjustment
account also exists so that hand-made movements cannot quietly pull the
valuation report away from the Inventory balance.

**`GET /inventory/stock/valuation` and the Inventory account are the same
number.** That is the invariant to reach for first when the books look wrong.

**Opening balances.** Stock already on the shelves was expensed on arrival, so
selling it under perpetual costing would credit an Inventory account that was
never debited. Migration `019` deliberately posts nothing — posted facts are
permanent — so squaring it is a one-time operator action, mirroring the
`unposted` / `post-unposted` pair: `GET /accounting/inventory-opening` previews
it, `POST` writes it, once, keyed so it cannot double.

It posts **Dr Inventory / Cr Cost of sales**, because those goods were already
expensed there and are still on hand: this reverses an over-expensing rather than
inventing an equity balance. One caveat, which the preview states on screen —
stock that arrived through a hand-made movement was never posted at all, so the
credit for that portion has nothing behind it.

`products.average_cost` is **derived, never entered**. The product form still
carries `cost_price` — a standing figure for what you expect to pay — and
editing it does not touch what the stock on the shelf cost. The two disagreeing
is normal and informative.

### Email verification

`users.email_verified` existed from the first migration and was never once set —
the schema advertised a confirmation step that did not exist. Registering now
sends a link, and clicking it sets the column.

The token is the same kind of credential as a password reset link and is handled
the same way: 256 bits of randomness, stored as a SHA-256 hash so a leaked backup
is not enough to confirm somebody else's address, single-use, and expiring — in
48 hours rather than the reset's 60 minutes, because there is nothing sensitive
behind it and a signup who verifies tomorrow should not need a new one.

**Nothing is gated on it.** Sign-in and every module work exactly as before,
verified or not. Requiring it would have locked out every account that existed
when this shipped, which is not a decision to make on an operator's behalf.
Unverified users see a dismissible prompt with a resend action; making it
un-dismissable would follow those older accounts around forever.

`POST /auth/resend-verification` answers identically whether the address is
unknown, already verified, or throttled — the same non-enumeration rule
`/auth/forgot-password` follows, and for the same reason. The throttle counts
the registration email too, so a resend in the first minute is deliberately a
no-op.

### Global search

`GET /api/v1/search?q=…` looks across fifteen kinds — contacts, companies,
opportunities, quotes, orders, invoices, products, warehouses, vendors, purchase
orders, projects, tasks, accounts, ledger entries and employees — and the header
search box drives it. `⌘K` focuses it, arrows and `Enter` pick.

The rule for what it matches: **global search searches exactly the fields each
module's own list filter already searches.** A term behaves the same whether it
is typed into the global box or a list screen, and adding a searchable field in
one place does not silently diverge from the other. `ILIKE`, like those filters.

One consequence worth knowing: there are no joins, so searching a customer's
name finds the *company*, not their invoices. The company's page links through
to its documents.

Results carry `kind` and `id` and **no URL** — route shapes belong to the
frontend, and a backend that emitted `/sales/invoices/{id}` would need
redeploying to rename a route. Each kind is capped at five hits so a catalogue
of matching products cannot bury the one matching invoice, and a term under two
characters returns nothing without touching the database.

**What a user may find depends on their role.** Accounts and ledger entries need
`accountant` or `manager`; employees need `hr` or `manager`. Kinds the caller
cannot see are never added to the query, so the database does not read rows that
would then be filtered away. The same term genuinely returns different sets to
different people rather than one of them getting a 403.

### Who can read what

Planning global search turned up that `auth_middleware` only ever
*authenticated* — authorization was per-handler, and only on the writes. Every
accounting read (accounts, ledger, bank accounts, tax rates, and the trial
balance, P&L and balance sheet) and the employee directory, which returns
`salary`, were readable by **any signed-in user**. The frontend hid those screens
behind `RoleRoute` and the OpenAPI tag claimed the module was restricted; the API
did not enforce it.

Those reads are now gated to match what the UI and the documentation already
said. This was a behaviour change: a plain user who could read them cannot any
more.

**Leave and expenses are scoped to the person**, which is a filter rather than a
gate: an ordinary employee must still file and read their own claims. Nothing in
HR checked ownership before — any signed-in user could read every claim in the
company, delete someone else's leave request, edit their draft expenses, and
file claims in their name.

The caller resolves once to one of three things: an `hr` or `manager` role sees
everyone; a login linked to an employee sees that employee; anything else sees
nothing. Lists force the `employee_id` filter that already existed. Someone
else's record answers **404 rather than 403**, because a 403 confirms it exists.
Filing in another person's name is **refused rather than quietly rewritten** to
the caller's own id — silently correcting a payload reports success for a request
nobody made.

That link is `employees.user_id`, which existed from the first HR migration
alongside a `find_by_user_id` lookup that was **called from nowhere**. Migration
`017` backfills it where exactly one user and one employee share an address —
ambiguous matches are left alone, because a wrong link hands somebody another
person's records — and the employee form gained a login field for the rest.
Employees with no login are ordinary; they simply have no self-service records.

### File upload

`expense_lines.receipt_url` had been in the schema since the first HR migration,
carried faithfully through the entity, the DTO and the repository — and nothing
in the product could ever set it. There was no upload endpoint, and the expense
form never rendered an input for it. So an approver deciding on a claim had
nothing to check the amount against. This is the third find of that shape, after
`users.email_verified` (never set) and `find_by_user_id` (never called).

Files live in **object storage**, spoken to over the S3 API, so the same code
addresses MinIO locally and a managed bucket in production. `docker compose up -d
minio` gives you a bucket on port 9000 with a console on
http://localhost:9001; the backend creates the bucket itself on first start.

| Variable | |
|---|---|
| `S3_ENDPOINT` | Setting it is what enables upload. Unset, uploads are refused |
| `S3_PUBLIC_ENDPOINT` | What a browser can reach; defaults to `S3_ENDPOINT` |
| `S3_BUCKET` | Defaults to `erp-receipts` |
| `S3_REGION` | Defaults to `us-east-1`, which is what MinIO expects |
| `S3_ACCESS_KEY` / `S3_SECRET_KEY` | Required once the endpoint is set |

Two endpoints, because a **presigned URL's signature covers the host it was
signed for**. Inside a compose network the API reaches MinIO as `minio:9000`
while the browser reaches it as `localhost:9000`; sign against the first and the
link is unusable. On a single host they are the same and the second can be
omitted.

- **Unconfigured storage refuses rather than pretends.** This is deliberately
  unlike the mail seam, where a missing relay falls back to writing the message
  to the log: a logged email is still a readable email, but a receipt accepted
  and dropped reports success and is gone, and nobody finds out until an auditor
  asks. The rest of the application is unaffected.
- **Uploads go through the API; reads do not.** `POST /files` takes the bytes;
  `GET /files/{id}` authorizes the caller and hands back a 15-minute presigned
  link. That keeps receipt traffic out of the API and is also what makes an
  `<img src>` work at all — the session is a bearer token in a header, and an
  image tag cannot send one.
- **The file type is decided by its leading bytes**, never by the `Content-Type`
  the upload claimed. The recorded type is handed back to a browser as the type
  to render, so trusting the client would let somebody store a script and have
  it served under our origin. JPEG, PNG, WebP and PDF; 10 MB.
- **Storage keys are generated**, `receipts/{yyyy}/{mm}/{uuid}.{ext}`, with
  nothing of the client's in them. The original filename is kept in the database
  for display and for `Content-Disposition` only, so `../` in a name reaches
  nothing.

**A receipt is readable by exactly whoever may read the claim it hangs off.** The
rule is inherited rather than invented: reading walks backwards from the file to
the expense line to the report, and asks the same `HrScope` the HR endpoints ask.
Someone else's answers 404, not 403. A file not yet attached to anything — the
window between choosing a file and saving the form — is readable only by whoever
uploaded it.

That inheritance has a second edge, easy to miss: attaching an id you did not
upload is **refused**, because attaching it to a claim you may read is precisely
what would make somebody else's file readable to you.

### Purchase returns

There was no return or credit-note concept anywhere in purchasing: an order could
be raised, received and paid for, and that was the whole story. Perpetual
inventory turned that from untidy into **wrong** — the only tool for a faulty
delivery was a hand-made stock adjustment, which relieved Inventory but debited
*Inventory adjustment* (an expense) and left `amount_due` untouched. Sending
goods back booked a loss **and** left you owing the supplier for stock you no
longer had.

A return is its own document, `PR-…`, mirroring the goods receipt field for
field. It records **quantities and no money**, because what a return is worth is
the purchase order's own line price.

That valuation is the crux rather than a convenience:

```
Dr Accounts payable   55.00      what the supplier credits
   Cr Inventory          55.00   what the receipt brought the goods in at
```

Relieving stock at the *current average* would credit Inventory 45.00 against a
55.00 debit and need a variance account to absorb the difference. Valuing both
legs at the price the goods arrived at makes them agree by construction, so there
is no variance and no thirteenth posting role. Under periodic costing the same
entry credits Cost of sales instead, exactly reversing what the receipt charged
there. The input tax goes back too — leaving it would reclaim tax on goods that
were returned.

**The average has to un-blend.** Taking stock off at its receipt cost while the
shelf carries a different average would pull the valuation report away from the
Inventory account, so `average_after_removal` recomputes it:

```
150 @ 4.5000 = 675.00,  send back 10 that came in at 5.50
-> 140 units worth 620.00,  average 4.4286
```

620 / 140 repeats, which is the case most likely to drift a cent — it does not,
and there is a test pinning that.

**The order expects the goods again.** A return decrements
`received_quantity`, so a fully received order drops back to partially received
and a replacement delivery is accepted. If none is coming, close it by hand. That
column is also why there is no separate "how much has gone back" query: it
already means "how many are here".

**Settlement counts both ledgers.** `amount_due` is `total − returned − paid`,
still derived rather than accumulated. Returning something already paid for takes
it **negative** — the supplier owes you, and it nets against the next purchase.
Getting the money back is a vendor refund, which does not exist yet.

Two things that are easy to get wrong and are handled: the movement a return
creates carries `reference_type = purchase_return` so `movement_entries` knows
the document already posted its own inventory leg and must not post a cost of
sale on top; and stock that has since been sold cannot be sent back, checked
before anything is written.

### Sales credit notes

There was no credit or refund concept anywhere in sales, and
`InvoiceStatus::can_transition` gives `paid` **no outgoing transitions at all**.
So a customer who paid and then sent two of ten items back left nothing to do:
no credit note, no partial adjustment, and no way to cancel. The only escape was
hand-writing a journal entry through `POST /accounting/ledger-entries`, tied to
nothing — not the invoice, not the stock, not `amount_paid`/`amount_due`.

A credit note (`CN-…`) mirrors the invoice, and **works on a paid invoice**
because it never touches the status machine: it recomputes settlement, and what
is now owed *back* shows as a negative `amount_due`. That is the same answer the
vendor side gives when you return something already paid for.

```
Dr Sales revenue   40.00      base_total − base_tax, as a remainder
Dr Tax payable      8.00
   Cr Accounts receivable  48.00
```

Revenue is the **remainder**, not a restated subtotal, for exactly the reason
issuing an invoice derives it that way: restating the subtotal on its own can
land a cent from the total, and the legs would then fail to add up to the
receivable they are relieving. Each line is credited at the invoice line's own
price, discount and tax rate, restated at the **invoice's** rate — the receivable
was raised at it.

**Goods coming back are optional and separate.** Name a warehouse and a second,
independent pair is posted:

```
Dr Inventory       16.00
   Cr Cost of sales  16.00
```

Leave it out and the credit is money only, which is what a price dispute or an
over-billing needs. What a customer is credited and what the goods cost are
unrelated numbers, which is why the two pairs never reference each other.

The goods return at the **current moving average** — they are physically
indistinguishable from what is already on the shelf, and weighted average has no
layer to identify them with. If the average has moved since the sale, the credit
to Cost of sales differs slightly from the debit the sale posted. That is
inherent to the method, and it is the opposite of a purchase return, where the
price is a documented fact and the average therefore *has* to un-blend.

Crediting is capped per line at the invoiced quantity less what has already been
credited. A purchase return needs no such tally because it decrements
`received_quantity`; invoice lines are immutable, so this query is the only thing
standing between a customer and being credited twice.

### Shipping on invoicing

Selling never moved stock. Receiving goods creates a movement automatically;
issuing an invoice did not, so the only way stock left the shelf was somebody
remembering to record a movement by hand. The credit note made that awkward — it
puts goods *back* automatically, reversing a movement nothing ever made.

Choosing a **dispatch warehouse** under `Settings → Company` switches it on.
Leave it empty and invoicing moves nothing, exactly as before.

This does **not** change where the cost of a sale posts. That still happens when
the stock movement is recorded; the movement simply now happens at issue, so
revenue and its cost land together by construction.

**Choosing a warehouse also switches on a refusal**, and that is the reason it is
opt-in rather than a default: issuing an invoice the shelf cannot cover is
refused with a 409 naming the SKU and what is available. The refusal happens
*before* the status is written, so the invoice stays a draft and nothing moves.
That ordering is the opposite of posting, and deliberately: having the stock is a
**precondition** of issuing, while posting is a **consequence** of it.

Cancelling an invoice brings the goods back. That needed one row added to the
movement rule — an inward movement caused by a sales invoice credits **Cost of
sales**, not Inventory adjustment. Without it a cancelled sale would be booked as
shrinkage and the cost of sale it reverses would be left standing.

Lines that hold no stock are skipped: a free-text delivery charge names no
product, and a `service` product was never on a shelf. Both are ordinary on an
invoice.

Invoices issued before a warehouse was chosen are left alone — a movement records
something physical happening, and nothing happened. Sales does not learn what a
warehouse is either: the seam is `shared::dispatch::StockDispatcher`, injected
like `DocumentPoster`, and the implementation reuses
`StockUseCases::record_movement` so the availability check, the moving average
and the posting are the same ones a person gets when recording a movement by
hand.

### Stock reservations

Once issuing an invoice took goods off the shelf, a confirmed order that reserved
nothing became a promise with nothing behind it: two orders could both be
confirmed against the last unit, and whichever was invoiced second was refused —
in front of a customer who had already been told they could have it.

`stock_levels.reserved_quantity` had been in the schema since the inventory
module was written. `available()` subtracts it, the low-stock query subtracts it,
and `record_movement` checks against it. **Nothing had ever written it.** The
column, the accessor and the consumer were all in place; only the writer was
missing.

Confirming an order now holds what the shelf can cover. Confirming one it
**cannot** cover still succeeds, reserving what is there and leaving the rest of
the promise unreserved — selling before buying is ordinary, and refusing would
block it outright. The shortfall is still caught, because issuing the invoice
refuses if the goods never arrived.

| Moment | What happens |
|---|---|
| draft → confirmed | reserve `min(available, ordered)` per stocked line |
| a confirmed order is edited | release, then reserve again |
| order cancelled | release |
| the order's invoice is issued | release, then ship |
| no dispatch warehouse set | nothing reserves, as nothing ships |

**Reservations are stored rows, not a figure derived from the order line**, and
that follows directly from reserving what is available: order 10 against 6 on the
shelf and 6 is held, so releasing has to give back exactly the 6 that was taken.
`stock_reservations` records it and `reserved_quantity` is the running total those
rows sum to, moved in the same transaction.

Two things worth knowing, both easy to get wrong:

- **The release before shipping.** An invoice's goods are reserved by the order it
  came from, and shipping checks what is *available* — so without giving the
  reservation back first, an order blocks its own shipment. The release happens
  immediately before the movements, which is why the invoice carries its
  `order_id`.
- **A confirmed order is editable**, and rewriting its lines replaces them.
  `stock_reservations.order_line_id` cascades from those lines, so the rows would
  be deleted by the write *without* giving the stock back. The release therefore
  happens before the lines go, not after.

Everything else comes for free. `available()` and the low-stock report already
read the column, so both start telling the truth the moment anything writes it —
the low-stock report in particular was reporting on quantity alone. And because
`record_movement` is the single door stock changes through, reserved goods are
protected from a hand-recorded movement without a line of new code.

### What `shipped` means

An order's status has to agree with where its goods actually are. Since issuing
an invoice is what takes them off the shelf, the rule is one sentence:

> A status that asserts the goods have left requires the order to be invoiced in
> full — when invoicing is what ships them.

`shipped` and `delivered` both assert it, so both are refused (409) while any
line still has an outstanding quantity, counted from invoices in `sent`, `paid`
or `overdue`. A **draft** invoice has shipped nothing and a **cancelled** one has
already had its goods put back, so neither counts.
`OrderStatus::asserts_goods_have_left` names the two statuses in the domain
rather than matching them inline.

It asks about *coverage*, not existence, and the difference is a whole class of
loss: a draft invoice is editable, so an operator short of stock would trim it to
what the shelf held and issue that — and the weaker rule let the order be marked
delivered anyway, with the rest unbilled and unrecorded. See "Billing an order in
instalments" below.

Without this an order could reach `delivered` — a terminal status — while its
goods sat on the shelf reserved forever, since only invoicing releases a
reservation. The inconsistent state is now simply unreachable; no stock mechanics
changed.

The clause "when invoicing is what ships them" is load-bearing. With **no
dispatch warehouse** set, invoicing moves no stock, there is no inconsistency to
prevent, and the rule does not exist — both statuses behave exactly as they did
before. Sales asks the dispatcher seam
(`StockDispatcher::ships_automatically`) rather than learning what a warehouse
is. The order detail says so up front instead of offering a button that 409s.

### What cancelling an invoice means

> Cancelling an invoice undoes issuing it — no more, and no less.

Each half of that sentence fixes something.

**No more.** Only an issued invoice has anything to unwind, so the whole
cancellation path is gated on `InvoiceStatus::is_issued` — the status the invoice
held *before* the write. Cancelling a **draft** now changes nothing but the
status. It used to run the full unwind: a reversal of a posting that never
happened, and an inward stock movement for goods that never went out, inventing
units and blending them into the moving average at the current cost.

**No less.** Issuing releases the order's reservation, so cancelling has to give
it back. The goods return to the shelf and then to the order's hold, in that
order — nothing can be held before it is there. Only for an order that
`still_expects_goods` (`confirmed` or `processing`); holding stock against a
cancelled order would strand it. What comes back is what is *available*, by the
same rule as confirming: if someone took the stock meanwhile, the order gets what
is left.

Two consequences follow:

- **An order gets one *live* invoice at a time, not one ever.** A cancelled
  invoice has been unwound on both sides, so it no longer stands as the order's
  invoice. Blocking on it left orders permanently unbillable — and once
  `shipped` required an issued invoice, permanently stuck in `processing` too.
  `find_invoices_for_order` returns them all, oldest first, and each caller
  applies its own predicate: `is_live` to find the one that still stands,
  `is_issued` to ask whether the goods actually went.
- **Cancelling is refused once the order says the goods left.** An order in
  `shipped` or `delivered` would be contradicted by its own invoice putting the
  goods back — the mirror of the state the lifecycle guard prevents. A **credit
  note** is the document for goods that have gone. Refused before the write, and
  only where invoicing ships: with no dispatch warehouse there is no shelf to
  contradict, and cancelling stays as unrestricted as it has always been.

### Billing an order in instalments

An order can be invoiced as many times as it takes; what it may not do is bill
the same units twice. `convert_to_invoice` takes optional per-line quantities and
refuses anything past a line's outstanding — the mirror of the over-receipt
refusal purchasing has always had, down to applying the running total locally so
a line named twice in one request is measured once. Omitting the quantities bills
everything outstanding, which is what the one-click conversion has always done.

**How much of a line is invoiced is derived, never stored.** `invoice_lines`
carries an `order_line_id`, and the answer is a sum over the order's live
invoices — the shape `credited_by_invoice_line` already uses to decide what an
invoice line has left to credit. That earns its keep three times over: cancelling
an invoice, deleting a draft and editing a draft's lines each give the quantity
back with no decrement path to write or forget. Purchasing stores a running
`received_quantity` instead, and can, because receiving has no cancellation to
unwind.

An order part-way through gets `partially_shipped`, set by the invoicing flow and
never requested — `OrderStatus::after_invoice` mirrors
`PurchaseOrderStatus::after_receipt`. Reaching `shipped` stays the operator's
call; what changed is that the lifecycle guard now lets them make it only when
nothing is outstanding.

**Reservations release by the line.** Shipping six of ten hands back six and
leaves the rest held, so `apply_movements` takes a list of
`ReservationRelease`s rather than an order id. The row is decremented and deleted
only at zero, which keeps `UNIQUE (order_line_id)` meaningful. Cancelling an
instalment releases the whole order and re-holds it, the way editing a confirmed
order already did — the cancelled units are outstanding again, so what the order
should hold is the whole of it, as far as the shelf reaches.

### What a draft invoice cannot do

A draft has raised no receivable, so **nothing can be settled against it**. It
cannot take a payment (`NotPayable`, 409) and it cannot be credited
(`NotCreditable`) — the same rule a draft purchase order already followed
(`NotPayable`, *"a draft has not been committed to, so there is nothing owed on
it yet"*). `InvoiceStatus::accepts_payment` states it positively: only `sent` and
`overdue` are live receivables.

Payment was the site that never got the rule, and it cost more than a wrong
balance. Money against a draft relieved a receivable that had never been raised —
cash in, a negative asset, and no revenue at all — and then the settlement write
carried the document somewhere it could not otherwise go:

- **paid in full** ⇒ `paid`, which is terminal. The invoice could never
  afterwards be issued (`draft → sent` is behind it) or cancelled
  (`paid → cancelled` is not a transition). Its revenue was unrecognisable for
  good.
- **part-paid** ⇒ `sent`, which is *issuing an invoice*. Except that shipping and
  posting hang off `set_status` and nothing else, so the goods stayed on the
  shelf and the books stayed empty — and since the order lifecycle guard asks
  whether an order has an **issued** invoice, the order could then be walked to
  `delivered` with its stock still reserved on the shelf. One currency unit was
  enough to defeat it.

So `settle_invoice` carries the rule too, beside the arm it already had for
cancelled invoices: **settlement never changes a draft's status**. Money must not
revive a closed document, and it must not issue an open one. That arm is
unreachable while both settling documents refuse drafts, which is the point — it
stays unreachable if a third one is ever added.

### A document's stock moves all at once, or not at all

`StockRepository::apply_movements` takes a whole document's movements and applies
them in one transaction. It is plural for a reason: shipping an invoice used to
release the order's hold and then move each line separately, so three things went
wrong at once.

- **A refusal dropped the hold.** The release committed, the movement failed, and
  an order that still expected ten units protected none of them.
- **A short line left the earlier lines gone.** Five units off the shelf and
  costed to Cost of sales, against an invoice that stayed a draft.
- **Retrying shipped them again.** Postings are idempotent through
  `posting_key`; movements have no equivalent, so the second attempt moved the
  already-moved lines a second time.

The release is part of that transaction too, because it and the movements are
one fact: the goods stop being held *and* they leave. It still happens first —
moving stock checks what is **available**, so without it an order blocks its own
shipment — but first *within* the transaction rather than before it.

**Availability is checked where the level is locked**, in the repository, not in
the use case. Checking earlier is both weaker and, now, wrong: weaker because two
shipments of the last unit can each read the level before either moves it; wrong
because a check that runs before the release sees the order's own goods as
unavailable. One consequence falls out for free — two lines of the same product
are now measured together, since by the second line the first has already taken
its units inside the transaction.

Four callers move stock this way: issuing or cancelling an invoice, a goods
receipt, a purchase return and a credit note. The two inward ones cannot fail for
want of stock, so what the plural call buys them is protection from a database
error partway through. Purchase returns check every line's availability before
writing their document, so their business-rule failure was already caught before
anything moved.

Postings still happen after the commit. That gap is the one this codebase
accepts and has machinery for — `posting_key`, the unposted report, the repair
endpoint. What changed is that a document's *stock* is now all-or-nothing.

### Percentages

Every rate and discount is a **whole percentage**: `20` means 20%. That holds on
document lines (`tax_rate`, `discount_percent`), on products, and in
`accounting.tax_rates.rate` alike. One shared validator
(`shared::validation::validate_percentage`) enforces 0–100, backed by CHECK
constraints on every column, so the two conventions cannot drift apart again.

### Money

Decimal columns are serialised as **strings** (`"1080.00"`), not numbers — JSON
numbers are IEEE-754 doubles and would quietly lose cents. The frontend formats
them for display and sends them back as fixed-scale strings; it never does
arithmetic on the value it received. Document totals are always computed on the
server (`shared::money`), with the browser showing a matching preview only.

### Document numbering

Quote, order, invoice, PO, receipt, expense and employee numbers come from
Postgres sequences via `next_document_number()` (e.g. `INV-2026-000001`), so
concurrent creates cannot collide on the UNIQUE constraint.

## License
MIT
