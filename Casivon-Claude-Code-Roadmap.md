# Casivon — Claude Code Development Roadmap

## Project Overview

You are continuing development of Casivon, a full-stack ERP built with:
- **Backend**: Rust (Axum) + PostgreSQL (SQLx) + Redis
- **Frontend**: Vite + React 18 + TypeScript + Tailwind CSS + Zustand + TanStack Query
- **Architecture**: Clean/Hexagonal Architecture (Domain → Application → Infrastructure)

The skeleton is partially built. Your job is to complete ALL modules to production quality.

---

## Current State

> Last verified: 2026-08-13. `cargo check --all-targets` and `cargo test` are
> clean (166 unit + 281 integration tests against live Postgres, Redis and Mailpit);
> `tsc`, `eslint --max-warnings 0` and `vite build` are clean (150 tests).

### Done
- [x] Project structure and Docker Compose
- [x] Auth (register, login, refresh with typed tokens, JWT middleware, RBAC,
      first-account-becomes-admin bootstrap, admin role grants)
- [x] CRM — contacts, companies, opportunities, activities: full CRUD, filters, pipeline report
- [x] Sales — quotes, orders, invoices, payments: full CRUD, quote→order→invoice
      conversion, payment settlement, overdue sweep
- [x] Inventory — products, categories, warehouses, stock levels/movements, BOMs,
      reorder alerts, valuation
- [x] Purchasing — vendors, POs with lines, goods receipts that post stock and
      advance the PO status
- [x] Accounting — chart of accounts (tree), double-entry ledger, bank accounts,
      tax rates, trial balance / P&L / balance sheet
- [x] HR — employees, leave with entitlement + overlap checks, expense reports with workflow
- [x] Projects — projects, task hierarchy + Kanban, time entries rolling up to progress
- [x] Database migrations for all 8 modules (009 fills the gaps the first eight left)
- [x] State machines for every document workflow, enforced server-side
- [x] Form validation — Zod on the frontend, `validator` on the backend, rules mirrored
- [x] Error handling, loading skeletons, empty states, toasts
- [x] Search, filtering, sorting (allow-listed) and pagination on every list endpoint
- [x] Role-based UI rendering (sidebar and route guards follow the API's own rules)
- [x] Dashboard fed by live data from every module
- [x] Tests — 100 backend unit tests, 172 backend integration tests against a real
      database (`#[sqlx::test]`, one throwaway database per test), 117 frontend
      tests (schemas, money maths, components, store, pages)
- [x] Sign-out — `POST /auth/logout` revokes the refresh token through a
      Redis denylist keyed by `jti` and expiring with the token; `/auth/refresh`
      checks it, and the frontend calls it before clearing local state
- [x] Password reset — one-hour single-use links, tokens stored hashed, no
      account enumeration, per-account send throttle, and a `session_epoch` that
      ends every existing session when the password changes
- [x] Settings — user administration (role grants, retire/restore, search,
      filter, sort), company profile, and a personal account screen with a
      change-password form
- [x] OpenAPI 3.1 — all 156 operations annotated with `utoipa`, Swagger UI at
      `/api/docs` and the document at `/api/v1/openapi.json`, with tests that
      fail if a route is added without documenting it
- [x] One tax-rate convention — a whole percentage everywhere (`20` means 20%),
      enforced by a shared validator and CHECK constraints; migration 012
      converts `tax_rates.rate` and adds the `tax_rate` column purchase order
      lines were missing
- [x] Frontend wire types generated from the OpenAPI document rather than
      hand-written, with tests on both links of the chain
- [x] A base currency taken from the organisation profile rather than five
      hardcoded `"USD"` constants, locked once documents exist
- [x] Scope-to-self for leave and expenses — nothing in HR checked ownership, so
      any signed-in user could read every claim, delete someone else's leave
      request, edit their draft expenses and file in their name. Now resolved
      per caller to all/own/nothing; 404 rather than 403 on a colleague's
      record, and filing in another name is refused. Migration 017 backfills the
      employee-to-login link by email where unambiguous
- [x] Global search — one query across fifteen kinds, searching exactly the
      fields each module's own list filter already searches, capped per kind and
      scoped to what the caller's role allows. Header box with ⌘K and keyboard
      navigation
- [x] Read authorization — `auth_middleware` only authenticated, so every
      accounting read and the employee directory (which returns salary) were
      open to any signed-in user while the UI and the OpenAPI tag both claimed
      otherwise. Found while planning search; now gated to match
- [x] Email verification — registering sends a single-use link, clicking it sets
      `users.email_verified`, which existed from the first migration and had
      never been set. Nothing is gated on it: sign-in works verified or not, and
      the prompt is dismissible, because every pre-existing account is
      unverified. Resend is throttled and does not reveal whether an address is
      registered
- [x] Vendor payments — purchasing could raise an order and receive goods but
      never pay for them; the mirror of sales payments, with the same
      no-overpayment rule, settlement derived from the payment ledger, and
      realised FX on a moved rate
- [x] Posting for the purchase and expense cycles — receiving goods books cost
      and a payable, paying a supplier clears it, approving an expense owes the
      employee and reimbursing settles it. Periodic costing, with input tax
      split out as recoverable. The profit and loss now has a cost side: before
      this it reported revenue with nothing against it, so every sale looked
      like pure profit
- [x] Automatic GL posting from sales documents — issuing an invoice books the
      revenue and the receivable, a payment clears it, and cancellations post
      mirrors rather than deleting. Switched on by mapping five accounts;
      unmapped installations post nothing and are unchanged. Every entry carries
      a unique posting key, so retrying cannot double revenue, and an unposted
      report plus a repair endpoint covers the gap between the document write
      and its entries
- [x] Multi-currency — documents carry the currency they were transacted in,
      the rate in force on their own date frozen onto the row, and the amount
      restated in the base currency. Effective-dated rates under Settings, every
      money aggregation moved onto the base column, realised FX gain/loss stored
      on each payment, and a currency with no rate refused rather than assumed
      to be parity
- [x] Mail transport — SMTP via `lettre`, selected at start-up by `SMTP_HOST`,
      falling back to the logging sender with a warning when unset; Mailpit in
      `docker-compose` and a delivery test that reads the message back

- [x] Receipt upload — files in S3-compatible object storage (MinIO in
      `docker-compose`), uploaded through the API and read back as a 15-minute
      presigned link so the browser fetches them directly. The type is decided by
      the leading bytes, not by what the upload claimed; storage keys are
      generated and contain nothing of the client's. A receipt is readable by
      exactly whoever may read the claim it hangs off, and attaching an id you
      did not upload is refused — that being what would otherwise turn the read
      rule inside out. Unconfigured storage refuses rather than accepting a file
      and dropping it

- [x] Perpetual inventory — stock is an asset on the balance sheet and becomes a
      cost when it leaves, instead of a cost the day it arrives. Moving weighted
      average per product, kept to four decimal places and written onto each
      movement so a later delivery cannot change what an earlier sale cost. The
      cost of a sale posts from the stock movement, since there is no automatic
      stock-out anywhere. Two new roles, and they are **optional** — making them
      required would have switched off all posting on every existing install.
      `GET /inventory/stock/valuation` and the Inventory account are now the same
      number, and `POST /accounting/inventory-opening` squares the one-time
      discontinuity for stock that was already expensed on arrival

- [x] Purchase returns — goods go back to the supplier as their own document,
      mirroring the goods receipt. Valued at the order's own line price, which is
      what makes the debit to payables and the credit to inventory agree by
      construction: no variance account, no thirteenth role. The average
      un-blends so the valuation report and the Inventory account stay the same
      number, input tax goes back with the goods, the order reopens for a
      replacement, and `amount_due` counts returns as well as payments — going
      negative when you return something already paid for

- [x] Sales credit notes — the sell-side counterpart of a purchase return, and
      the answer to a case that had none: `paid` has no outgoing status
      transition, so a paid invoice could be neither cancelled nor adjusted. A
      credit note never touches the status machine — it recomputes settlement,
      and what is owed back shows as a negative `amount_due`. Revenue comes off
      as the remainder so the legs still add up to the receivable, and goods
      coming back are an optional second pair, posted only when a warehouse is
      named and only under perpetual costing

- [x] Shipping on invoicing — choosing a dispatch warehouse makes issuing an
      invoice take the goods off the shelf, so revenue and its cost land
      together. Unset means nothing moves, which is how every installation
      starts; choosing one also makes an invoice the shelf cannot cover refuse to
      issue, leaving it a draft. Cancelling brings the goods back as a credit to
      Cost of sales rather than as shrinkage

- [x] Stock reservations — confirming an order holds what the shelf can cover, so
      two customers cannot be promised the same unit. Confirming short of stock
      still confirms, reserving what is there; the shortfall is caught when the
      invoice is issued. Reservations are stored rows because the amount held is
      not the amount ordered, and `reserved_quantity` — read by `available()` and
      the low-stock query since the inventory module was written, and written by
      nothing until now — is the running total they sum to

- [x] The order lifecycle agrees with where stock moves — `shipped` and
      `delivered` assert the goods have left, so both are refused until the
      order's invoice has been issued, on installations where issuing is what
      ships. Before this an order could reach `delivered` — terminal — with its
      goods still on the shelf and reserved forever, since only invoicing
      releases a reservation. With no dispatch warehouse the rule does not exist
      and both statuses behave as they always did

- [x] Cancelling an invoice undoes issuing it — no more, and no less. Cancelling
      a **draft** now does nothing, where it used to reverse a posting that never
      happened and bring in stock that never went out, inventing goods and
      blending them into the moving average. Cancelling an **issued** invoice
      gives the order its reservation back along with the goods, no longer
      blocks the order from being invoiced again, and is refused outright once
      the order says the goods reached the customer — a credit note is the
      document for that

- [x] A draft invoice takes no money. It relieved a receivable that had never
      been raised, and the settlement write then carried it somewhere
      `set_status` would not: paid in full it reached `paid`, terminal, with its
      revenue unrecognisable for good; part-paid it reached `sent` without
      shipping or posting anything, which let the order it came from be marked
      delivered with its stock still on the shelf. One currency unit defeated the
      lifecycle guard. Payments now follow the rule vendor payments and credit
      notes already did, and settlement never changes a draft's status

- [x] A document's stock moves atomically. Shipping an invoice released the
      order's hold and then moved each line in its own transaction, so a refusal
      left the hold gone, a short line left the earlier lines already off the
      shelf and costed against a draft, and retrying moved them a second time.
      One `apply_movements` call now covers the release and every line, and
      availability is checked where the stock level is locked — which also makes
      two simultaneous shipments of the last unit safe, and measures two lines of
      the same product together

- [x] Partial fulfilment — an order can be billed in instalments, the mirror of
      the goods receipts purchasing has always had. How much of a line is
      invoiced is *derived* from the live invoice lines pointing at it, so
      cancelling an invoice, deleting a draft or editing one gives the quantity
      back with nothing to remember; `partially_shipped` is set by the flow; and
      the lifecycle guard now asks whether every line is invoiced rather than
      whether an invoice exists. Before this, an operator short of stock would
      trim the draft invoice to what the shelf held and end up with an order
      terminally `delivered` for ten units with six billed — the other four
      unbillable and unrecorded

### Still open
- [ ] Attachments beyond expense lines — the `attachments` table and the
      `/files` endpoints are generic, but only expense lines reference them.
      Invoices with a signed delivery note, purchase orders with a supplier
      quotation
- [ ] A reaper for uploads that were never attached. They are readable only by
      whoever uploaded them, so they are inert — they just cost storage
- [ ] Gantt view for projects (the Kanban board is built)
- [ ] Unrealised FX revaluation of open balances. Now that postings exist it is
      only blocked on a fiscal period and a close procedure — there is still no
      defined moment at which "revalue what is still outstanding" would run
- [ ] Per-category expense accounts. `expense_lines.category` exists and every
      claim currently posts to one general expense account
- [ ] FIFO as an alternative to weighted average, for jurisdictions that want it
- [ ] Per-line or per-invoice dispatch warehouses. One default for the
      organisation is deliberately blunt; a picker on the document is the
      obvious next step if it proves too much so
- [ ] A screen showing what a given product is reserved for — the obvious thing
      to want now that `stock_reservations` records it per order line
- [ ] Picking and packing documents, and a shipment separate from the invoice —
      partial fulfilment is done, but the invoice is still what moves the goods
- [ ] Back-orders as their own record with a promised date. A line's outstanding
      quantity is the back-order today, and nothing commits to when it arrives
- [ ] Landed costs — freight capitalised into stock value rather than expensed
      on arrival
- [ ] Payroll
- [ ] Refunds on both sides, where money actually moves rather than netting
      off the next document
- [ ] Vendor credit notes for a price dispute. The sales side already covers
      it — a credit note with no warehouse credits money and moves nothing
- [ ] Price lists — one product, many prices, each with its own currency and
      validity window. `products` has no currency column, so product prices are
      base-currency by definition today. Bolting a single `currency` column onto
      `products` would let a catalogue hold one EUR product and one USD product
      with no coherent way to show a customer either list

### Known quirks
- Signing out revokes the refresh token but not the access token issued with it,
  which stays valid for up to 15 minutes. Closing that window means a denylist
  lookup on every authenticated request, making Redis a hard dependency of all
  traffic rather than of `/auth/refresh` alone.
- Money is serialised as a string, but the scale is not stable at zero: sqlx
  decodes a Postgres `numeric(15, 2)` zero as scale 0, so a settled invoice
  reports `"0"` where every other amount reads `"0.00"`. The frontend parses
  before formatting, so this is cosmetic on the wire only.

---

## Architecture Rules (MUST FOLLOW)

### Backend — Clean Architecture

```
modules/{module_name}/
├── domain/
│   ├── mod.rs
│   ├── entities.rs          # Pure structs, no framework deps
│   ├── repositories.rs      # Traits only (async_trait)
│   └── errors.rs            # Module-specific errors
├── application/
│   ├── mod.rs
│   ├── dto.rs               # Request/response structs with validator
│   └── use_cases.rs         # Business logic, orchestrates repos
├── infrastructure/
│   ├── mod.rs
│   └── repositories/
│       ├── mod.rs
│       └── {entity}_repo.rs  # SQLx implementations
├── handlers.rs              # Axum route handlers (thin!)
└── routes.rs                # Router definitions
```

**Rules:**
1. Handlers must be THIN — only extract request, call use case, return response
2. Use cases contain ALL business logic
3. Entities are pure Rust structs with `sqlx::FromRow` derive
4. Repository traits use `async-trait`
5. DTOs use `validator` crate with `#[derive(Validate)]`
6. NEVER put SQL in handlers
7. NEVER put business logic in repositories
8. Use `AppError` from `src/error.rs` for all errors
9. All list endpoints MUST support pagination (`PaginationParams`)
10. All create/update endpoints MUST validate with `validator`
11. Use `CurrentUser` extension for auth context
12. All timestamps: `DateTime<Utc>` for created_at/updated_at, `NaiveDate` for business dates
13. Money: use `rust_decimal::Decimal` (NOT float)

### Frontend — Component Architecture

```
src/
├── api/
│   └── client.ts            # Axios instance with interceptors
├── components/
│   ├── ui/                  # Reusable primitives (Button, Input, Card, etc.)
│   ├── layout/              # Sidebar, Header, AppLayout
│   └── forms/               # Module-specific forms
├── pages/
│   ├── Dashboard.tsx
│   └── {module}/
│       ├── {Module}List.tsx
│       ├── {Module}Detail.tsx
│       └── {Module}Form.tsx
├── hooks/
│   └── use{Module}.ts       # TanStack Query hooks
├── store/
│   └── authStore.ts
└── types/
    └── index.ts
```

**Rules:**
1. Use TanStack Query for ALL server state (`useQuery`, `useMutation`)
2. Use Zustand for client state ONLY (auth, UI preferences)
3. Forms use `react-hook-form` + `zod` resolver
4. All API calls go through `src/api/client.ts`
5. Loading states: skeleton screens, not spinners
6. Error states: toast notifications + inline errors
7. Tables: sortable columns, pagination, row actions
8. Use `lucide-react` for ALL icons
9. Follow existing component patterns (cn() utility, class-variance-authority)
10. Responsive: sidebar collapses on mobile, tables scroll horizontally

---

## Module Implementation Order

### Phase 1: Foundation (Do First)
1. **Fix Auth module gaps**
   - Refresh token logic with Redis blacklist
   - Password reset flow
   - Email verification
   - Role-based access control middleware

2. **Add rust_decimal dependency**
   - Add to Cargo.toml: `rust_decimal = { version = "1.33", features = ["db-postgres"] }`
   - Fix all entity files that reference `rust_decimal::Decimal`

3. **Add missing Cargo dependencies**
   - `rust_decimal` for money fields
   - `utoipa` + `utoipa-swagger-ui` for API docs (optional but recommended)

### Phase 2: Core Business Modules

#### Module 2.1: SALES
**Entities:** Quote, QuoteLine, SalesOrder, OrderLine, Invoice, InvoiceLine, Payment
**Workflow:** Quote (draft → sent → accepted) → SalesOrder (confirmed) → Invoice (sent → paid)
**Features:**
- Create quote with line items
- Convert quote to order
- Convert order to invoice
- Record payments against invoices
- Auto-calculate totals, tax, discounts
- Invoice status: draft, sent, paid, overdue, cancelled
- Payment methods: bank_transfer, credit_card, cash, check, stripe, paypal

**Frontend Pages:**
- `/sales/quotes` — List with status filters
- `/sales/quotes/new` — Create quote form with dynamic line items
- `/sales/quotes/:id` — Detail view with "Convert to Order" button
- `/sales/orders` — List
- `/sales/orders/:id` — Detail with "Create Invoice" button
- `/sales/invoices` — List with payment status
- `/sales/invoices/:id` — Detail + "Record Payment" modal
- `/sales/payments` — Payment history

#### Module 2.2: INVENTORY
**Entities:** Product, ProductCategory, Warehouse, StockLevel, StockMovement, BillOfMaterials, BomLine
**Features:**
- Product catalog with variants
- Multi-warehouse stock tracking
- Stock movements (in, out, transfer, adjustment)
- Auto-update stock levels on sales/purchase events
- Reorder level alerts
- Bill of Materials for manufacturing
- Barcode support

**Frontend Pages:**
- `/inventory/products` — Product grid with search
- `/inventory/products/new` — Product form
- `/inventory/products/:id` — Detail + stock levels per warehouse
- `/inventory/warehouses` — Warehouse list
- `/inventory/movements` — Stock movement log
- `/inventory/boms` — BOM list
- `/inventory/boms/new` — BOM builder with component picker

#### Module 2.3: PURCHASING
**Entities:** Vendor, PurchaseOrder, PurchaseOrderLine, GoodsReceipt, GoodsReceiptLine
**Workflow:** PO (draft → sent → confirmed) → GoodsReceipt → Auto-update stock
**Features:**
- Vendor directory
- Create PO with line items
- Goods receipt against PO lines
- Partial receipt support
- Auto-close PO when fully received

**Frontend Pages:**
- `/purchasing/vendors` — Vendor directory
- `/purchasing/vendors/new` — Vendor form
- `/purchasing/purchase-orders` — PO list with status
- `/purchasing/purchase-orders/new` — PO creation
- `/purchasing/purchase-orders/:id` — Detail + "Create Receipt" button
- `/purchasing/goods-receipts` — Receipt log

### Phase 3: Financial & HR

#### Module 3.1: ACCOUNTING
**Entities:** Account, GeneralLedgerEntry, BankAccount, TaxRate
**Features:**
- Chart of accounts (hierarchical)
- Double-entry bookkeeping (every transaction = debit + credit)
- Bank reconciliation
- Tax rate management
- Financial reports: P&L, Balance Sheet, Trial Balance
- Multi-currency support

**Frontend Pages:**
- `/accounting/accounts` — Chart of accounts tree
- `/accounting/accounts/new` — Account form
- `/accounting/ledger` — General ledger entries
- `/accounting/ledger/new` — Journal entry form (debit/credit lines)
- `/accounting/bank-accounts` — Bank accounts
- `/accounting/tax-rates` — Tax configuration
- `/accounting/reports` — P&L, Balance Sheet views

#### Module 3.2: HR
**Entities:** Employee, LeaveRequest, ExpenseReport, ExpenseLine
**Workflows:**
- Leave: pending → approved/rejected
- Expense: draft → submitted → approved → reimbursed
**Features:**
- Employee directory linked to users
- Leave balance tracking
- Expense receipt upload (URL for now, file storage later)
- Approval workflows

**Frontend Pages:**
- `/hr/employees` — Employee directory
- `/hr/employees/new` — Employee form
- `/hr/employees/:id` — Profile + leave balance
- `/hr/leave-requests` — Leave calendar + list
- `/hr/leave-requests/new` — Leave request form
- `/hr/expense-reports` — Expense report list
- `/hr/expense-reports/new` — Expense form with line items + receipt upload

#### Module 3.3: PROJECTS
**Entities:** Project, Task, TimeEntry
**Features:**
- Project creation with budget tracking
- Task hierarchy (parent/child tasks)
- Gantt chart view (use library or custom)
- Time tracking per task
- Project progress auto-calculation
- Billable vs non-billable time

**Frontend Pages:**
- `/projects` — Project list with progress bars
- `/projects/new` — Project form
- `/projects/:id` — Project detail + task list
- `/projects/:id/tasks` — Task board (Kanban style)
- `/projects/:id/tasks/new` — Task form
- `/projects/:id/time-entries` — Time log
- `/projects/:id/time-entries/new` — Time entry form

### Phase 4: Polish

1. **Dashboard widgets**
   - Revenue chart (Recharts)
   - Pending invoices count
   - Low stock alerts
   - Upcoming tasks
   - Recent activities feed

2. **Global search**
   - Search across contacts, companies, products, invoices
   - Quick results dropdown

3. **Settings**
   - Organization profile
   - User management (admin only)
   - Currency settings
   - Tax configuration
   - Email templates

4. **Notifications**
   - In-app notification bell
   - Toast system for actions

5. **Reports**
   - Sales report by period
   - Inventory valuation
   - Employee timesheet report
   - Aged receivables

---

## Database Conventions

1. **Primary keys**: `UUID` with `uuid_generate_v4()`
2. **Timestamps**: `TIMESTAMPTZ` for `created_at`, `updated_at`
3. **Business dates**: `DATE` (NaiveDate in Rust)
4. **Money**: `DECIMAL(15, 2)`
5. **Percentages**: `DECIMAL(5, 4)` for tax rates (0.2000 = 20%)
6. **Soft deletes**: Use `is_active` boolean or `deleted_at` timestamp (choose one, be consistent)
7. **Foreign keys**: Always add `ON DELETE` behavior
8. **Indexes**: Add indexes on all foreign keys and frequently queried fields
9. **Updated at trigger**: Use the existing `update_updated_at_column()` function
10. **Constraints**: Use CHECK constraints for enums (e.g., `CHECK (status IN ('draft', 'sent', 'paid'))`)

---

## API Conventions

1. **Base path**: `/api/v1`
2. **Resources**: Plural nouns (`/contacts`, `/purchase-orders`)
3. **Methods**:
   - `POST /{resource}` — Create
   - `GET /{resource}` — List (with pagination)
   - `GET /{resource}/:id` — Get one
   - `PUT /{resource}/:id` — Update
   - `DELETE /{resource}/:id` — Delete
4. **Query params**:
   - `?page=1&per_page=20` — Pagination
   - `?status=active&name=john` — Filtering
   - `?sort=-created_at` — Sorting (minus = desc)
5. **Response format**:
   ```json
   {
     "success": true,
     "data": { ... },
     "pagination": { "page": 1, "per_page": 20, "total": 100, "total_pages": 5 }
   }
   ```
6. **Error format**:
   ```json
   {
     "success": false,
     "error": { "code": 422, "message": "Validation failed" }
   }
   ```

---

## State Machines (Implement These)

### Quote
```
draft → sent → [accepted | rejected | expired]
```

### SalesOrder
```
draft → confirmed → processing → [partially_shipped →] shipped → delivered
        ↓
     cancelled
```
`partially_shipped` is set by the invoicing flow, never requested — the mirror of
`partially_received` on a purchase order. `shipped` and `delivered` additionally
require every line to be invoiced, where invoicing is what takes the goods off
the shelf — see "What `shipped` means" in the README.

### Invoice
```
draft → sent → [paid | overdue] → cancelled
```

### PurchaseOrder
```
draft → sent → confirmed → [partially_received | fully_received] → closed
        ↓
     cancelled
```

### LeaveRequest
```
pending → [approved | rejected]
```

### ExpenseReport
```
draft → submitted → [approved | rejected] → reimbursed
```

### Task
```
todo → in_progress → review → done
   ↓
cancelled
```

---

## Testing Strategy

### Backend
1. **Unit tests**: Test use cases with mock repositories
2. **Integration tests**: Test handlers with `axum::TestClient`
3. **Database tests**: Use `sqlx::test` macro with test transactions

### Frontend
1. **Component tests**: Vitest + React Testing Library
2. **API mocking**: MSW (Mock Service Worker) for API tests
3. **E2E**: Playwright (optional, add later)

---

## Code Quality Checklist

Before marking any module complete, verify:

- [ ] All entities have proper `sqlx::FromRow` derives
- [ ] All DTOs have `validator` derives with proper rules
- [ ] All handlers return proper `AppResult<Json<T>>`
- [ ] All list endpoints support pagination
- [ ] All routes are registered in `app.rs`
- [ ] Frontend has loading, error, and empty states
- [ ] Forms validate with Zod before submission
- [ ] TypeScript types match backend DTOs exactly
- [ ] No `any` types in TypeScript (use `unknown` if needed)
- [ ] All SQL queries use parameterized queries (SQLx does this automatically)
- [ ] No SQL injection vectors
- [ ] Proper error messages shown to users
- [ ] Responsive design works on mobile

---

## Commands You'll Use

```bash
# Backend
cd backend
cargo sqlx migrate add create_{table_name}    # New migration
cargo sqlx migrate run                          # Run migrations
cargo sqlx prepare                              # Generate offline query data
cargo check                                     # Fast compile check
cargo test                                      # Run tests
cargo run                                       # Start server

# Frontend
cd frontend
npm install                                     # Install deps
npm run dev                                     # Start dev server
npm run build                                   # Production build
npm run lint                                    # Check linting
```

---

## IMPORTANT NOTES

1. **Do NOT change the existing architecture** — extend it, don't refactor it
2. **Follow existing patterns** — copy the CRM module structure for new modules
3. **Use the existing components** — Button, Input, Card, etc. are already built
4. **Keep handlers thin** — business logic goes in use cases
5. **Test as you go** — don't write 1000 lines without testing
6. **Commit frequently** — use git commits after each module
7. **Ask if stuck** — but try to solve it using the existing patterns first

---

## Success Criteria

The project is complete when:
- All 8 modules have full CRUD backend + frontend
- Dashboard shows real data from all modules
- Users can log in, navigate, and perform core business operations
- Forms validate properly with clear error messages
- The UI is responsive and polished
- No compilation errors in Rust or TypeScript
- Docker Compose starts everything successfully
