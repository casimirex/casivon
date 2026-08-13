import { http } from '@/api/client';
import type { DocumentBody } from './useResource';
import { createResource, useAction } from './useResource';
import type {
  CreditNote,
  CreditNoteDetail,
  Invoice,
  InvoiceDetail,
  OrderDetail,
  Payment,
  Quote,
  QuoteDetail,
  SalesOrder,
  Uuid,
} from '@/types';

export const quotes = createResource<Quote, QuoteDetail, DocumentBody, DocumentBody>('/sales/quotes', 'quotes');
export const orders = createResource<SalesOrder, OrderDetail, DocumentBody, DocumentBody>('/sales/orders', 'orders');
export const invoices = createResource<Invoice, InvoiceDetail, DocumentBody, DocumentBody>('/sales/invoices', 'invoices');
export const payments = createResource<Payment>('/sales/payments', 'payments');

/** Drives the status buttons on the quote detail page. */
export const creditNotes = createResource<CreditNote, CreditNoteDetail>(
  '/sales/credit-notes',
  'credit-notes'
);

export interface CreateCreditNoteInput {
  invoice_id: Uuid;
  warehouse_id?: Uuid;
  issue_date?: string;
  reason?: string;
  notes?: string;
  lines: Array<{ invoice_line_id: Uuid; quantity: number }>;
}

/**
 * Crediting changes what the invoice is owed, and — when a warehouse is named —
 * puts stock back, so everything downstream of both refreshes.
 */
export function useCreateCreditNote() {
  return useAction<CreditNoteDetail, CreateCreditNoteInput>(
    'credit-notes',
    (body) => http.post<CreditNoteDetail>('/sales/credit-notes', body),
    {
      successMessage: 'Credit note issued',
      invalidateKeys: ['invoices', 'products', 'movements', 'stock', 'ledger-entries'],
    }
  );
}

export function useQuoteStatus() {
  return useAction<Quote, { id: Uuid; status: string }>(
    'quotes',
    ({ id, status }) => http.put<Quote>(`/sales/quotes/${id}/status`, { status }),
    { successMessage: 'Quote updated' }
  );
}

export function useConvertQuoteToOrder() {
  return useAction<OrderDetail, { id: Uuid }>(
    'quotes',
    ({ id }) => http.post<OrderDetail>(`/sales/quotes/${id}/convert-to-order`, {}),
    { successMessage: 'Sales order created from quote', invalidateKeys: ['orders'] }
  );
}

export function useOrderStatus() {
  return useAction<SalesOrder, { id: Uuid; status: string }>(
    'orders',
    ({ id, status }) => http.put<SalesOrder>(`/sales/orders/${id}/status`, { status }),
    { successMessage: 'Order updated' }
  );
}

export function useConvertOrderToInvoice() {
  return useAction<
    InvoiceDetail,
    {
      id: Uuid;
      payment_terms_days?: number;
      /** Omitted bills everything still outstanding. */
      lines?: Array<{ order_line_id: Uuid; quantity: number }>;
    }
  >(
    'orders',
    ({ id, payment_terms_days, lines }) =>
      http.post<InvoiceDetail>(`/sales/orders/${id}/convert-to-invoice`, {
        payment_terms_days,
        lines,
      }),
    { successMessage: 'Invoice raised from order', invalidateKeys: ['invoices'] }
  );
}

export function useInvoiceStatus() {
  return useAction<Invoice, { id: Uuid; status: string }>(
    'invoices',
    ({ id, status }) => http.put<Invoice>(`/sales/invoices/${id}/status`, { status }),
    { successMessage: 'Invoice updated' }
  );
}

export interface RecordPaymentInput {
  invoice_id: Uuid;
  amount: string;
  payment_method: string;
  payment_date: string;
  reference?: string;
  notes?: string;
}

/** Recording a payment re-settles the invoice, so both caches are refreshed. */
export function useRecordPayment() {
  return useAction<Payment, RecordPaymentInput>(
    'payments',
    (body) => http.post<Payment>('/sales/payments', body),
    { successMessage: 'Payment recorded', invalidateKeys: ['invoices'] }
  );
}

export function useDeletePayment() {
  return useAction<unknown, Uuid>(
    'payments',
    (id) => http.delete(`/sales/payments/${id}`),
    { successMessage: 'Payment reversed', invalidateKeys: ['invoices'] }
  );
}
