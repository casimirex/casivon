import { z } from 'zod';
import {
  currencyCode,
  enumOf,
  isoDate,
  money,
  optionalCurrency,
  optionalEmail,
  optionalIsoDate,
  optionalMoney,
  optionalPercent,
  optionalString,
  optionalUrl,
  optionalUuid,
  percent,
  positiveInt,
  requiredEmail,
  requiredString,
  requiredUuid,
} from './common';
import {
  ACCOUNT_TYPES,
  ACTIVITY_TYPES,
  COMPANY_TYPES,
  CONTACT_STATUSES,
  EMPLOYEE_STATUSES,
  LEAVE_TYPES,
  MOVEMENT_TYPES,
  OPPORTUNITY_STAGES,
  PAYMENT_METHODS,
  PRIORITIES,
  PRODUCT_TYPES,
} from '@/types';

// --------------------------------------------------------------------- crm

export const contactSchema = z.object({
  first_name: requiredString('First name', 100),
  last_name: requiredString('Last name', 100),
  email: optionalEmail,
  phone: optionalString,
  mobile: optionalString,
  job_title: optionalString,
  company_id: optionalUuid,
  status: enumOf(CONTACT_STATUSES, 'status'),
  city: optionalString,
  country: optionalString,
  address: optionalString,
  notes: optionalString,
});
export type ContactForm = z.input<typeof contactSchema>;

export const companySchema = z.object({
  name: requiredString('Company name'),
  legal_name: optionalString,
  tax_id: optionalString,
  company_type: enumOf(COMPANY_TYPES, 'company type'),
  email: optionalEmail,
  phone: optionalString,
  website: optionalUrl,
  industry: optionalString,
  city: optionalString,
  country: optionalString,
  address: optionalString,
});
export type CompanyForm = z.input<typeof companySchema>;

export const opportunitySchema = z.object({
  title: requiredString('Title'),
  company_id: requiredUuid('Customer'),
  contact_id: optionalUuid,
  stage: enumOf(OPPORTUNITY_STAGES, 'stage'),
  value: optionalMoney('Value'),
  currency: optionalCurrency,
  probability: z.coerce.number().int().min(0, 'Cannot be below 0').max(100, 'Cannot exceed 100').optional(),
  expected_close_date: optionalIsoDate,
  source: optionalString,
  description: optionalString,
});
export type OpportunityForm = z.input<typeof opportunitySchema>;

export const activitySchema = z.object({
  activity_type: enumOf(ACTIVITY_TYPES, 'activity type'),
  subject: requiredString('Subject'),
  description: optionalString,
  related_to_type: optionalString,
  related_to_id: optionalUuid,
});
export type ActivityForm = z.input<typeof activitySchema>;

// ------------------------------------------------------------------- sales

/** Quote, order and invoice lines are the same shape on the wire. */
export const documentLineSchema = z.object({
  product_id: optionalUuid,
  description: requiredString('Description', 1000),
  quantity: positiveInt('Quantity'),
  unit_price: money('Unit price'),
  discount_percent: percent('Discount').default(0),
  tax_rate: percent('Tax rate').default(0),
});
export type DocumentLineForm = z.input<typeof documentLineSchema>;

const withLines = z.array(documentLineSchema).min(1, 'Add at least one line item');

export const quoteSchema = z
  .object({
    customer_id: requiredUuid('Customer'),
    contact_id: optionalUuid,
    issue_date: isoDate('Issue date'),
    expiry_date: isoDate('Expiry date'),
    currency: optionalCurrency,
    notes: optionalString,
    terms: optionalString,
    lines: withLines,
  })
  // Mirrors the SalesError::ExpiryBeforeIssue check on the server.
  .refine((data) => data.expiry_date >= data.issue_date, {
    message: 'Expiry date cannot be before the issue date',
    path: ['expiry_date'],
  });
export type QuoteForm = z.input<typeof quoteSchema>;

export const orderSchema = z.object({
  customer_id: requiredUuid('Customer'),
  contact_id: optionalUuid,
  order_date: isoDate('Order date'),
  required_date: optionalIsoDate,
  shipping_address: optionalString,
  billing_address: optionalString,
  currency: optionalCurrency,
  notes: optionalString,
  lines: withLines,
});
export type OrderForm = z.input<typeof orderSchema>;

export const invoiceSchema = z
  .object({
    customer_id: requiredUuid('Customer'),
    order_id: optionalUuid,
    issue_date: isoDate('Issue date'),
    due_date: isoDate('Due date'),
    currency: optionalCurrency,
    notes: optionalString,
    lines: withLines,
  })
  .refine((data) => data.due_date >= data.issue_date, {
    message: 'Due date cannot be before the issue date',
    path: ['due_date'],
  });
export type InvoiceForm = z.input<typeof invoiceSchema>;

/** `maxAmount` is the invoice's outstanding balance — the server refuses more. */
export const paymentSchema = (maxAmount?: number) =>
  z.object({
    amount: money('Amount', { min: 0.01 }).refine(
      (value) => maxAmount === undefined || value <= maxAmount,
      { message: `Cannot exceed the ${maxAmount?.toFixed(2)} still outstanding` }
    ),
    payment_method: enumOf(PAYMENT_METHODS, 'payment method'),
    payment_date: isoDate('Payment date'),
    reference: optionalString,
    notes: optionalString,
  });
export type PaymentForm = z.input<ReturnType<typeof paymentSchema>>;

/// Each line is capped at what is still uncredited on that invoice line, which
/// is what the server checks too.
export const creditNoteSchema = z.object({
  warehouse_id: optionalUuid,
  issue_date: optionalIsoDate,
  reason: optionalString,
  notes: optionalString,
  lines: z
    .array(
      z.object({
        invoice_line_id: z.string(),
        description: z.string(),
        creditable: z.number(),
        quantity: z.coerce.number().int().min(0),
      })
    )
    .superRefine((lines, ctx) => {
      lines.forEach((line, index) => {
        if (line.quantity > line.creditable) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            message: `Only ${line.creditable} left to credit`,
            path: [index, 'quantity'],
          });
        }
      });

      if (lines.every((line) => Number(line.quantity) === 0)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: 'Credit at least one item',
          path: [0, 'quantity'],
        });
      }
    }),
});
export type CreditNoteForm = z.input<typeof creditNoteSchema>;

// --------------------------------------------------------------- inventory

export const productSchema = z.object({
  sku: requiredString('SKU', 100),
  name: requiredString('Product name'),
  description: optionalString,
  product_type: enumOf(PRODUCT_TYPES, 'product type'),
  category_id: optionalUuid,
  unit_of_measure: optionalString,
  cost_price: optionalMoney('Cost price'),
  sale_price: optionalMoney('Sale price'),
  // A percentage, not an amount — 20 means 20%.
  tax_rate: optionalPercent('Tax rate'),
  barcode: optionalString,
  weight: optionalMoney('Weight'),
  dimensions: optionalString,
});
export type ProductForm = z.input<typeof productSchema>;

export const warehouseSchema = z.object({
  code: requiredString('Code', 50),
  name: requiredString('Warehouse name', 100),
  address: optionalString,
  city: optionalString,
  country: optionalString,
});
export type WarehouseForm = z.input<typeof warehouseSchema>;

export const movementSchema = z
  .object({
    product_id: requiredUuid('Product'),
    warehouse_id: requiredUuid('Warehouse'),
    to_warehouse_id: optionalUuid,
    movement_type: enumOf(MOVEMENT_TYPES, 'movement type'),
    quantity: z.coerce
      .number({ invalid_type_error: 'Quantity must be a number' })
      .int('Quantity must be a whole number')
      .refine((value) => value !== 0, 'Quantity cannot be zero'),
    unit_cost: optionalMoney('Unit cost'),
    notes: optionalString,
  })
  .superRefine((data, ctx) => {
    if (data.movement_type === 'transfer') {
      if (!data.to_warehouse_id) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: 'A transfer needs a destination warehouse',
          path: ['to_warehouse_id'],
        });
      } else if (data.to_warehouse_id === data.warehouse_id) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: 'Source and destination must differ',
          path: ['to_warehouse_id'],
        });
      }
    }

    // Only adjustments may write stock down with a negative figure.
    if (data.quantity < 0 && data.movement_type !== 'adjustment') {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: 'Only an adjustment may use a negative quantity',
        path: ['quantity'],
      });
    }
  });
export type MovementForm = z.input<typeof movementSchema>;

export const bomLineSchema = z.object({
  component_id: requiredUuid('Component'),
  quantity_required: positiveInt('Quantity'),
  unit_of_measure: optionalString,
});

export const bomSchema = z
  .object({
    product_id: requiredUuid('Product'),
    version: optionalString,
    quantity_to_produce: positiveInt('Quantity to produce').default(1),
    lines: z.array(bomLineSchema).min(1, 'Add at least one component'),
  })
  .refine((data) => !data.lines.some((line) => line.component_id === data.product_id), {
    message: 'A product cannot be a component of itself',
    path: ['lines'],
  });
export type BomForm = z.input<typeof bomSchema>;

// -------------------------------------------------------------- purchasing

export const vendorSchema = z.object({
  name: requiredString('Vendor name'),
  legal_name: optionalString,
  tax_id: optionalString,
  email: optionalEmail,
  phone: optionalString,
  address: optionalString,
  city: optionalString,
  country: optionalString,
  payment_terms: optionalString,
  currency: optionalCurrency,
});
export type VendorForm = z.input<typeof vendorSchema>;

export const purchaseOrderLineSchema = z.object({
  product_id: optionalUuid,
  description: requiredString('Description', 1000),
  quantity: positiveInt('Quantity'),
  unit_price: money('Unit price'),
  tax_rate: percent('Tax rate').default(0),
});

export const purchaseOrderSchema = z.object({
  vendor_id: requiredUuid('Vendor'),
  order_date: isoDate('Order date'),
  expected_date: optionalIsoDate,
  shipping_address: optionalString,
  currency: optionalCurrency,
  notes: optionalString,
  lines: z.array(purchaseOrderLineSchema).min(1, 'Add at least one line item'),
});
export type PurchaseOrderForm = z.input<typeof purchaseOrderSchema>;

export const goodsReceiptSchema = z.object({
  warehouse_id: requiredUuid('Warehouse'),
  receipt_date: optionalIsoDate,
  notes: optionalString,
  lines: z
    .array(
      z.object({
        po_line_id: z.string(),
        description: z.string(),
        outstanding: z.number(),
        quantity_received: z.coerce.number().int().min(0),
      })
    )
    // Each line is capped at what is still outstanding, matching the server.
    .superRefine((lines, ctx) => {
      lines.forEach((line, index) => {
        if (line.quantity_received > line.outstanding) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            message: `Only ${line.outstanding} still outstanding`,
            path: [index, 'quantity_received'],
          });
        }
      });

      if (!lines.some((line) => line.quantity_received > 0)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: 'Receive at least one item',
          path: [0, 'quantity_received'],
        });
      }
    }),
});
export type GoodsReceiptForm = z.input<typeof goodsReceiptSchema>;

/// The mirror of `goodsReceiptSchema`, capped at what is on the shelf rather
/// than at what is outstanding — you can only send back what arrived.
export const purchaseReturnSchema = z.object({
  warehouse_id: requiredUuid('Warehouse'),
  return_date: optionalIsoDate,
  reason: optionalString,
  notes: optionalString,
  lines: z
    .array(
      z.object({
        po_line_id: z.string(),
        description: z.string(),
        received: z.number(),
        quantity_returned: z.coerce.number().int().min(0),
      })
    )
    .superRefine((lines, ctx) => {
      lines.forEach((line, index) => {
        if (line.quantity_returned > line.received) {
          ctx.addIssue({
            code: z.ZodIssueCode.custom,
            message: `Only ${line.received} were received`,
            path: [index, 'quantity_returned'],
          });
        }
      });

      if (lines.every((line) => Number(line.quantity_returned) === 0)) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: 'Send back at least one item',
          path: [0, 'quantity_returned'],
        });
      }
    }),
});
export type PurchaseReturnForm = z.input<typeof purchaseReturnSchema>;

// -------------------------------------------------------------- accounting

export const accountSchema = z.object({
  account_code: requiredString('Account code', 50),
  account_name: requiredString('Account name'),
  account_type: enumOf(ACCOUNT_TYPES, 'account type'),
  parent_id: optionalUuid,
  is_bank_account: z.boolean().default(false),
  currency: optionalCurrency,
  opening_balance: optionalMoney('Opening balance'),
});
export type AccountForm = z.input<typeof accountSchema>;

export const ledgerEntrySchema = z
  .object({
    entry_date: isoDate('Entry date'),
    description: requiredString('Description', 1000),
    debit_account_id: requiredUuid('Debit account'),
    credit_account_id: requiredUuid('Credit account'),
    amount: money('Amount', { min: 0.01 }),
    currency: optionalCurrency,
  })
  // Double entry: an entry that debits and credits one account is a no-op.
  .refine((data) => data.debit_account_id !== data.credit_account_id, {
    message: 'Debit and credit must be different accounts',
    path: ['credit_account_id'],
  });
export type LedgerEntryForm = z.input<typeof ledgerEntrySchema>;

export const bankAccountSchema = z.object({
  account_id: requiredUuid('Ledger account'),
  bank_name: requiredString('Bank name'),
  account_number: requiredString('Account number', 100),
  iban: optionalString,
  swift: optionalString,
  branch: optionalString,
});
export type BankAccountForm = z.input<typeof bankAccountSchema>;

export const taxRateSchema = z.object({
  name: requiredString('Name', 100),
  // A whole percentage, the same convention as `tax_rate` on a document line.
  rate: percent('Rate'),
  tax_type: requiredString('Tax type', 50),
  country: optionalString,
});
export type TaxRateForm = z.input<typeof taxRateSchema>;

// ---------------------------------------------------------------------- hr

export const employeeSchema = z.object({
  employee_number: optionalString,
  first_name: requiredString('First name', 100),
  last_name: requiredString('Last name', 100),
  email: requiredEmail,
  phone: optionalString,
  hire_date: isoDate('Hire date'),
  department: optionalString,
  job_title: optionalString,
  manager_id: optionalUuid,
  /// Which login this person signs in with. What makes their leave and
  /// expenses theirs — an unlinked employee has no self-service records.
  user_id: optionalUuid,
  salary: optionalMoney('Salary'),
  currency: optionalCurrency,
  annual_leave_entitlement: z.coerce
    .number()
    .int()
    .min(0, 'Cannot be negative')
    .max(365, 'Cannot exceed 365 days')
    .default(25),
  status: enumOf(EMPLOYEE_STATUSES, 'status').optional(),
});
export type EmployeeForm = z.input<typeof employeeSchema>;

export const leaveRequestSchema = z
  .object({
    employee_id: requiredUuid('Employee'),
    leave_type: enumOf(LEAVE_TYPES, 'leave type'),
    start_date: isoDate('Start date'),
    end_date: isoDate('End date'),
    days_requested: z.coerce.number().int().min(1, 'At least one day').optional(),
    reason: optionalString,
  })
  .refine((data) => data.end_date >= data.start_date, {
    message: 'Leave must end on or after it starts',
    path: ['end_date'],
  });
export type LeaveRequestForm = z.input<typeof leaveRequestSchema>;

export const expenseLineSchema = z.object({
  expense_date: isoDate('Date'),
  category: requiredString('Category', 100),
  description: requiredString('Description', 1000),
  amount: money('Amount', { min: 0.01 }),
  // The id returned by the upload. `receipt_url` is still accepted by the API
  // for an existing client, but the form no longer offers it — nothing in this
  // application could ever produce a URL to put in it.
  receipt_attachment_id: z.string().uuid().nullish(),
});

export const expenseReportSchema = z.object({
  employee_id: requiredUuid('Employee'),
  description: optionalString,
  currency: optionalCurrency,
  lines: z.array(expenseLineSchema).min(1, 'Add at least one expense line'),
});
export type ExpenseReportForm = z.input<typeof expenseReportSchema>;

// ---------------------------------------------------------------- projects

export const projectSchema = z
  .object({
    project_code: optionalString,
    name: requiredString('Project name'),
    description: optionalString,
    customer_id: optionalUuid,
    manager_id: optionalUuid,
    priority: enumOf(PRIORITIES, 'priority').default('medium'),
    start_date: optionalIsoDate,
    end_date: optionalIsoDate,
    budget: optionalMoney('Budget'),
    currency: optionalCurrency,
  })
  .refine((data) => !data.start_date || !data.end_date || data.end_date >= data.start_date, {
    message: 'End date cannot be before the start date',
    path: ['end_date'],
  });
export type ProjectForm = z.input<typeof projectSchema>;

export const taskSchema = z
  .object({
    project_id: requiredUuid('Project'),
    parent_task_id: optionalUuid,
    task_code: optionalString,
    title: requiredString('Title'),
    description: optionalString,
    assigned_to: optionalUuid,
    priority: enumOf(PRIORITIES, 'priority').default('medium'),
    start_date: optionalIsoDate,
    due_date: optionalIsoDate,
    estimated_hours: optionalMoney('Estimated hours'),
  })
  .refine((data) => !data.start_date || !data.due_date || data.due_date >= data.start_date, {
    message: 'Due date cannot be before the start date',
    path: ['due_date'],
  });
export type TaskForm = z.input<typeof taskSchema>;

export const timeEntrySchema = z.object({
  task_id: requiredUuid('Task'),
  employee_id: requiredUuid('Employee'),
  entry_date: isoDate('Date'),
  // The server refuses more than 24 hours against a single day.
  hours: z.coerce
    .number({ invalid_type_error: 'Hours must be a number' })
    .gt(0, 'Log more than zero hours')
    .max(24, 'A single entry cannot exceed 24 hours'),
  description: optionalString,
  is_billable: z.boolean().default(true),
});
export type TimeEntryForm = z.input<typeof timeEntrySchema>;

// --------------------------------------------------------------------- auth

export const loginSchema = z.object({
  email: requiredEmail,
  password: z.string().min(1, 'Password is required'),
});
export type LoginForm = z.infer<typeof loginSchema>;

export const registerSchema = z.object({
  first_name: requiredString('First name', 100),
  last_name: requiredString('Last name', 100),
  email: requiredEmail,
  // Matches the 8-character minimum enforced by the RegisterRequest DTO.
  password: z.string().min(8, 'Password must be at least 8 characters'),
});
export type RegisterForm = z.infer<typeof registerSchema>;

export const forgotPasswordSchema = z.object({
  email: requiredEmail,
});
export type ForgotPasswordForm = z.infer<typeof forgotPasswordSchema>;

export const resetPasswordSchema = z
  .object({
    // Same minimum as RegisterRequest — a reset is not a way around it.
    password: z.string().min(8, 'Password must be at least 8 characters'),
    confirm_password: z.string().min(1, 'Confirm your new password'),
  })
  // Confirmation is a frontend concern: the API takes one password, and the
  // point of asking twice is to catch a typo before the old one stops working.
  .refine((values) => values.password === values.confirm_password, {
    message: 'Passwords do not match',
    path: ['confirm_password'],
  });
export type ResetPasswordForm = z.infer<typeof resetPasswordSchema>;

export const profileSchema = z.object({
  first_name: requiredString('First name', 100),
  last_name: requiredString('Last name', 100),
});
export type ProfileForm = z.infer<typeof profileSchema>;

export const changePasswordSchema = z
  .object({
    current_password: z.string().min(1, 'Current password is required'),
    new_password: z.string().min(8, 'Password must be at least 8 characters'),
    confirm_password: z.string().min(1, 'Confirm your new password'),
  })
  .refine((values) => values.new_password === values.confirm_password, {
    message: 'Passwords do not match',
    path: ['confirm_password'],
  })
  .refine((values) => values.new_password !== values.current_password, {
    message: 'The new password must be different',
    path: ['new_password'],
  });
export type ChangePasswordForm = z.infer<typeof changePasswordSchema>;

/**
 * A field the form may legitimately submit empty.
 *
 * Deliberately not `optionalString`, which turns `''` into `undefined` and so
 * drops the key from the payload. The organisation endpoint reads an omitted
 * field as "leave it alone" and `''` as "clear it" — dropping the key would
 * make clearing a field impossible.
 */
const clearable = (max: number) =>
  z.string().trim().max(max, `Must be ${max} characters or fewer`);

export const organizationSchema = z.object({
  name: requiredString('Company name', 200),
  legal_name: clearable(200),
  email: z.union([z.literal(''), z.string().trim().email('Enter a valid email address')]),
  phone: clearable(50),
  website: z.union([
    z.literal(''),
    z.string().trim().url('Enter a valid URL, including https://'),
  ]),
  tax_number: clearable(50),
  address_line1: clearable(200),
  address_line2: clearable(200),
  city: clearable(100),
  postal_code: clearable(20),
  country: clearable(100),
  default_currency: currencyCode,
  // A warehouse id, or '' meaning "do not ship automatically" — the same
  // convention every other clearable field on this form uses.
  default_dispatch_warehouse_id: z.union([z.literal(''), z.string().uuid()]),
});
export type OrganizationForm = z.infer<typeof organizationSchema>;

export { currencyCode, toPayload } from './common';
