/**
 * Wire types.
 *
 * The entity types below are **generated**, not written: each one aliases a
 * schema in `src/api/schema.d.ts`, which `npm run generate:types` produces from
 * the backend's own OpenAPI document. Editing a Rust DTO and forgetting the
 * frontend is now a type error rather than a runtime surprise.
 *
 * What stays hand-written is what the document cannot express: the semantic
 * string aliases below, the generic response envelope (the generated wrappers
 * are one concrete type per payload), and the constant arrays, which are
 * runtime values rather than types.
 */

import type { components } from '@/api/schema';

type Schemas = components['schemas'];

// ------------------------------------------------------------------ aliases

export type Uuid = string;
/** DECIMAL(15,2) on the wire. Format with `formatMoney`, never with `+`. */
export type Money = string;
/** `YYYY-MM-DD` */
export type IsoDate = string;
/** RFC 3339 timestamp */
export type IsoDateTime = string;

// ------------------------------------------------------------- api envelope
//
// Hand-written because these are generic over their payload. The document has
// to name a concrete type per response (`ApiResponse_Contact`), which is no use
// to a client that unwraps the envelope before the caller ever sees it.

export interface ApiResponse<T> {
  success: true;
  data: T;
}

export type PaginationMeta = Schemas['PaginationMeta'];

export interface PaginatedResponse<T> {
  success: true;
  data: T[];
  pagination: PaginationMeta;
}

export interface ApiErrorBody {
  success: false;
  error: Schemas['ErrorDetail'];
}

export interface ListParams {
  page?: number;
  per_page?: number;
  /** `-created_at` sorts descending. */
  sort?: string;
  [key: string]: string | number | boolean | undefined;
}

// ------------------------------------------------------------------- settings

export type OrganizationSettings = Schemas['OrganizationSettings'];
export type FxRate = Schemas['FxRate'];
export type PostingConfiguration = Schemas['PostingConfiguration'];
export type PostingAccounts = Schemas['PostingAccounts'];
export type UnpostedReport = Schemas['UnpostedReport'];
export type UnpostedDocument = Schemas['UnpostedDocument'];
export type PostingRunReport = Schemas['PostingRunReport'];
export type InventoryOpeningReport = Schemas['InventoryOpeningReport'];
export type StockOnHand = Schemas['StockOnHand'];
export type AvailableCurrencies = Schemas['AvailableCurrencies'];

// ----------------------------------------------------------------------- auth

/** The full record, as `/users/me` and the user list return it. */
export type User = Schemas['UserProfile'];

/**
 * The trimmed user embedded in an auth response, and therefore what a session
 * actually knows about the signed-in person.
 *
 * Sign-in does not return `is_active` or `created_at`; the hand-written types
 * this file replaced marked them optional on one `User` type, which hid the
 * difference. A `User` is assignable here, so passing the fuller record from
 * `/users/me` still works.
 */
export type SessionUser = Schemas['UserResponse'];

export type AuthResponse = Schemas['AuthResponse'];

// ------------------------------------------------------------------------ crm

export type Contact = Schemas['Contact'];
export type Company = Schemas['Company'];
export type Opportunity = Schemas['Opportunity'];
export type Activity = Schemas['Activity'];
export type PipelineStage = Schemas['PipelineStage'];

// ---------------------------------------------------------------------- sales

/** Quote, order and invoice lines share this shape on the wire. */
export type DocumentLine = Omit<Schemas['QuoteLine'], 'quote_id'>;

export type QuoteLine = Schemas['QuoteLine'];
export type OrderLine = Schemas['OrderLine'];
export type InvoiceLine = Schemas['InvoiceLine'];
export type Quote = Schemas['Quote'];
export type QuoteDetail = Schemas['QuoteDetail'];
export type SalesOrder = Schemas['SalesOrder'];
export type OrderDetail = Schemas['OrderDetail'];
export type Invoice = Schemas['Invoice'];
export type Payment = Schemas['Payment'];
export type InvoiceDetail = Schemas['InvoiceDetail'];
export type CreditNote = Schemas['CreditNote'];
export type CreditNoteLine = Schemas['CreditNoteLine'];
export type CreditNoteDetail = Schemas['CreditNoteDetail'];

// ------------------------------------------------------------------ inventory

export type ProductCategory = Schemas['ProductCategory'];
export type Product = Schemas['Product'];
export type ProductDetail = Schemas['ProductDetail'];
export type Warehouse = Schemas['Warehouse'];
export type StockLevelView = Schemas['StockLevelView'];
export type StockMovement = Schemas['StockMovement'];
export type MovementResult = Schemas['MovementResult'];
export type ValuationResponse = Schemas['ValuationResponse'];
export type BillOfMaterials = Schemas['BillOfMaterials'];
export type BomLine = Schemas['BomLine'];
export type BomDetail = Schemas['BomDetail'];

// ----------------------------------------------------------------- purchasing

export type Vendor = Schemas['Vendor'];
export type PurchaseOrder = Schemas['PurchaseOrder'];
export type PurchaseOrderLineView = Schemas['PurchaseOrderLineView'];
export type PurchaseOrderDetail = Schemas['PurchaseOrderDetail'];
export type GoodsReceipt = Schemas['GoodsReceipt'];
export type GoodsReceiptLine = Schemas['GoodsReceiptLine'];
export type GoodsReceiptDetail = Schemas['GoodsReceiptDetail'];
export type PurchaseReturn = Schemas['PurchaseReturn'];
export type PurchaseReturnLine = Schemas['PurchaseReturnLine'];
export type PurchaseReturnDetail = Schemas['PurchaseReturnDetail'];
export type VendorPayment = Schemas['VendorPayment'];

// --------------------------------------------------------------------- search

export type SearchHit = Schemas['SearchHit'];
export type SearchResults = Schemas['SearchResults'];

// ---------------------------------------------------------------------- files

export type AttachmentSummary = Schemas['AttachmentSummary'];
export type AttachmentLink = Schemas['AttachmentLink'];

// ----------------------------------------------------------------- accounting

export type Account = Schemas['Account'];
export type AccountNode = Schemas['AccountNode'];
export type AccountBalance = Schemas['AccountBalance'];
export type GeneralLedgerEntry = Schemas['GeneralLedgerEntry'];
export type BankAccount = Schemas['BankAccount'];
export type TaxRate = Schemas['TaxRate'];
export type TrialBalanceReport = Schemas['TrialBalanceReport'];
export type ProfitAndLossReport = Schemas['ProfitAndLossReport'];
export type BalanceSheetReport = Schemas['BalanceSheetReport'];

// ------------------------------------------------------------------------- hr

export type Employee = Schemas['Employee'];
export type EmployeeDetail = Schemas['EmployeeDetail'];
export type LeaveRequest = Schemas['LeaveRequest'];
export type LeaveBalance = Schemas['LeaveBalance'];
export type ExpenseLine = Schemas['ExpenseLine'];
export type ExpenseReport = Schemas['ExpenseReport'];
export type ExpenseReportDetail = Schemas['ExpenseReportDetail'];

// ------------------------------------------------------------------- projects

export type Project = Schemas['Project'];
export type ProjectDetail = Schemas['ProjectDetail'];
export type Task = Schemas['Task'];
export type TaskSummary = Schemas['TaskSummary'];
export type TaskWithProject = Schemas['TaskWithProject'];
export type TimeEntry = Schemas['TimeEntry'];

// ------------------------------------------------------------ constant values

// Runtime values, so they cannot come from a type-only document. Each mirrors
// the matching `const` array in the Rust DTOs.

/** Mirrors `USER_ROLES` in the auth DTOs. */
export const USER_ROLES = ['admin', 'manager', 'accountant', 'hr', 'sales', 'user'] as const;
export type UserRole = (typeof USER_ROLES)[number];

export const CONTACT_STATUSES = ['lead', 'prospect', 'customer', 'supplier'] as const;

export const COMPANY_TYPES = ['customer', 'supplier', 'prospect', 'partner'] as const;

export const OPPORTUNITY_STAGES = [
  'prospecting',
  'qualification',
  'proposal',
  'negotiation',
  'closed_won',
  'closed_lost',
] as const;

export const ACTIVITY_TYPES = ['call', 'meeting', 'email', 'note', 'task'] as const;

export const QUOTE_STATUSES = ['draft', 'sent', 'accepted', 'rejected', 'expired'] as const;

export const ORDER_STATUSES = [
  'draft',
  'confirmed',
  'processing',
  'partially_shipped',
  'shipped',
  'delivered',
  'cancelled',
] as const;

export const INVOICE_STATUSES = ['draft', 'sent', 'paid', 'overdue', 'cancelled'] as const;

export const PAYMENT_METHODS = [
  'bank_transfer',
  'credit_card',
  'cash',
  'check',
  'stripe',
  'paypal',
] as const;

export const PRODUCT_TYPES = ['product', 'service', 'raw_material'] as const;

export const MOVEMENT_TYPES = ['in', 'out', 'transfer', 'adjustment'] as const;

export const PO_STATUSES = [
  'draft',
  'sent',
  'confirmed',
  'partially_received',
  'fully_received',
  'closed',
  'cancelled',
] as const;

export const ACCOUNT_TYPES = ['asset', 'liability', 'equity', 'revenue', 'expense'] as const;

export const EMPLOYEE_STATUSES = ['active', 'on_leave', 'terminated'] as const;

export const LEAVE_TYPES = ['annual', 'sick', 'maternity', 'paternity', 'unpaid'] as const;

export const LEAVE_STATUSES = ['pending', 'approved', 'rejected'] as const;

export const EXPENSE_STATUSES = [
  'draft',
  'submitted',
  'approved',
  'rejected',
  'reimbursed',
] as const;

export const PROJECT_STATUSES = [
  'planning',
  'active',
  'on_hold',
  'completed',
  'cancelled',
] as const;

export const TASK_STATUSES = ['todo', 'in_progress', 'review', 'done', 'cancelled'] as const;

export const PRIORITIES = ['low', 'medium', 'high', 'urgent'] as const;
