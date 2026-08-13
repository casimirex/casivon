import { http } from '@/api/client';
import type { DocumentBody } from './useResource';
import { createResource, useAction } from './useResource';
import type {
  GoodsReceipt,
  GoodsReceiptDetail,
  PurchaseReturn,
  PurchaseReturnDetail,
  PurchaseOrder,
  PurchaseOrderDetail,
  Uuid,
  Vendor,
  VendorPayment,
} from '@/types';

export const vendors = createResource<Vendor>('/purchasing/vendors', 'vendors');

/**
 * Money going out against a purchase order.
 *
 * Recording or reversing one moves the order's settlement figures and the
 * ledger, so both invalidate the order it belongs to as well as the list.
 */
export const vendorPayments = createResource<VendorPayment>(
  '/purchasing/vendor-payments',
  'vendor-payments'
);
export const purchaseOrders = createResource<PurchaseOrder, PurchaseOrderDetail, DocumentBody, DocumentBody>(
  '/purchasing/purchase-orders',
  'purchase-orders'
);
export const goodsReceipts = createResource<GoodsReceipt, GoodsReceiptDetail>(
  '/purchasing/goods-receipts',
  'goods-receipts'
);

export function usePurchaseOrderStatus() {
  return useAction<PurchaseOrder, { id: Uuid; status: string }>(
    'purchase-orders',
    ({ id, status }) =>
      http.put<PurchaseOrder>(`/purchasing/purchase-orders/${id}/status`, { status }),
    { successMessage: 'Purchase order updated' }
  );
}

export interface CreateReceiptInput {
  po_id: Uuid;
  warehouse_id: Uuid;
  receipt_date?: string;
  notes?: string;
  lines: Array<{ po_line_id: Uuid; quantity_received: number; notes?: string }>;
}

/** Receiving goods moves stock and advances the PO, so three caches refresh. */
export function useCreateReceipt() {
  return useAction<GoodsReceiptDetail, CreateReceiptInput>(
    'goods-receipts',
    (body) => http.post<GoodsReceiptDetail>('/purchasing/goods-receipts', body),
    {
      successMessage: 'Goods received into stock',
      invalidateKeys: ['purchase-orders', 'products', 'movements', 'stock'],
    }
  );
}

export const purchaseReturns = createResource<PurchaseReturn, PurchaseReturnDetail>(
  '/purchasing/purchase-returns',
  'purchase-returns'
);

export interface CreateReturnInput {
  po_id: Uuid;
  warehouse_id: Uuid;
  return_date?: string;
  reason?: string;
  notes?: string;
  lines: Array<{ po_line_id: Uuid; quantity_returned: number; notes?: string }>;
}

/**
 * Sending goods back moves stock, credits the ledger *and* reduces what the
 * order says is owed — so everything the receipt invalidates, plus the order
 * itself for the settlement figures.
 */
export function useCreateReturn() {
  return useAction<PurchaseReturnDetail, CreateReturnInput>(
    'purchase-returns',
    (body) => http.post<PurchaseReturnDetail>('/purchasing/purchase-returns', body),
    {
      successMessage: 'Goods sent back and credited',
      invalidateKeys: ['purchase-orders', 'products', 'movements', 'stock', 'ledger-entries'],
    }
  );
}

export function useVendorOptions() {
  const { data, isLoading } = vendors.useList({ per_page: 200, status: 'active', sort: 'name' });
  return {
    isLoading,
    options: (data?.data ?? []).map((vendor) => ({ value: vendor.id, label: vendor.name })),
  };
}
