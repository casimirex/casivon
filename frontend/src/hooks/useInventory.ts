import { useQuery } from '@tanstack/react-query';
import { http } from '@/api/client';
import type { DocumentBody } from './useResource';
import { createResource, useAction } from './useResource';
import type {
  BillOfMaterials,
  BomDetail,
  MovementResult,
  Product,
  ProductCategory,
  ProductDetail,
  StockLevelView,
  StockMovement,
  ValuationResponse,
  Warehouse,
  PaginatedResponse,
} from '@/types';

export const products = createResource<Product, ProductDetail>('/inventory/products', 'products');
export const warehouses = createResource<Warehouse>('/inventory/warehouses', 'warehouses');
export const boms = createResource<BillOfMaterials, BomDetail, DocumentBody, DocumentBody>('/inventory/boms', 'boms');
export const movements = createResource<StockMovement>('/inventory/movements', 'movements');

export function useCategories() {
  return useQuery({
    queryKey: ['categories'],
    queryFn: () => http.get<ProductCategory[]>('/inventory/categories'),
  });
}

/** Stock movements are the only way levels change, so both caches refresh. */
export function useRecordMovement() {
  return useAction<MovementResult, Record<string, unknown>>(
    'movements',
    (body) => http.post<MovementResult>('/inventory/movements', body),
    {
      successMessage: 'Stock movement recorded',
      invalidateKeys: ['products', 'stock', 'warehouses'],
    }
  );
}

export function useLowStock(params?: { page?: number; per_page?: number }) {
  return useQuery({
    queryKey: ['stock', 'low', params ?? {}],
    queryFn: () => http.list<StockLevelView>('/inventory/stock/low', params),
  });
}

export function useStockValuation() {
  return useQuery({
    queryKey: ['stock', 'valuation'],
    queryFn: () => http.get<ValuationResponse>('/inventory/stock/valuation'),
  });
}

export function useWarehouseStock(warehouseId: string | undefined, page = 1) {
  return useQuery<PaginatedResponse<StockLevelView>>({
    queryKey: ['stock', 'warehouse', warehouseId, page],
    queryFn: () =>
      http.list<StockLevelView>(`/inventory/warehouses/${warehouseId}/stock`, { page }),
    enabled: Boolean(warehouseId),
  });
}

export function useProductOptions() {
  const { data, isLoading } = products.useList({ per_page: 200, is_active: true, sort: 'name' });
  return {
    isLoading,
    products: data?.data ?? [],
    options: (data?.data ?? []).map((product) => ({
      value: product.id,
      label: `${product.sku} — ${product.name}`,
    })),
  };
}

export function useWarehouseOptions() {
  const { data, isLoading } = warehouses.useList({ per_page: 200, is_active: true });
  return {
    isLoading,
    options: (data?.data ?? []).map((warehouse) => ({
      value: warehouse.id,
      label: `${warehouse.code} — ${warehouse.name}`,
    })),
  };
}
