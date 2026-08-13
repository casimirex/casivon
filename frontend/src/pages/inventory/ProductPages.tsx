import { useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { AlertTriangle, Pencil, Plus } from 'lucide-react';
import { PageHeader } from '@/components/common/PageHeader';
import { DataTable, type Column } from '@/components/common/DataTable';
import { FilterBar } from '@/components/common/Filters';
import { SummaryGrid } from '@/components/common/DocumentView';
import { Field, FormGrid } from '@/components/common/Field';
import { DetailSkeleton } from '@/components/ui/Skeleton';
import { EmptyState, ErrorState } from '@/components/common/States';
import { Button } from '@/components/ui/Button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { Textarea } from '@/components/ui/Textarea';
import { Dialog } from '@/components/ui/Dialog';
import { Badge, StatusBadge } from '@/components/ui/Badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/Table';
import { useListParams } from '@/hooks/useListParams';
import { products, useCategories } from '@/hooks/useInventory';
import { formatMoney, formatNumber } from '@/lib/utils';
import { toPayload } from '@/schemas/common';
import { productSchema, type ProductForm as ProductFormValues } from '@/schemas';
import { PRODUCT_TYPES, type Product } from '@/types';

export function ProductList() {
  const navigate = useNavigate();
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams({
    search: '',
    product_type: '',
  });
  const query = products.useList(params);
  const [creating, setCreating] = useState(false);

  const columns: Column<Product>[] = [
    {
      key: 'sku',
      header: 'SKU',
      sortable: true,
      render: (row) => <span className="font-mono text-xs font-medium">{row.sku}</span>,
    },
    {
      key: 'name',
      header: 'Product',
      sortable: true,
      render: (row) => (
        <div>
          <p className="font-medium text-slate-900">{row.name}</p>
          <p className="text-xs text-slate-500">{row.unit_of_measure}</p>
        </div>
      ),
    },
    {
      key: 'product_type',
      header: 'Type',
      render: (row) => <StatusBadge status={row.product_type} />,
    },
    {
      key: 'cost_price',
      header: 'Cost',
      align: 'right',
      render: (row) => <span className="tabular-nums">{formatMoney(row.cost_price)}</span>,
    },
    {
      key: 'sale_price',
      header: 'Price',
      sortable: true,
      align: 'right',
      render: (row) => (
        <span className="font-medium tabular-nums">{formatMoney(row.sale_price)}</span>
      ),
    },
    {
      key: 'is_active',
      header: 'Active',
      render: (row) =>
        row.is_active ? <Badge tone="success">Active</Badge> : <Badge tone="muted">Inactive</Badge>,
    },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Products"
        description="The catalogue behind quotes, orders and stock"
        actions={
          <Button onClick={() => setCreating(true)}>
            <Plus className="mr-1 h-4 w-4" />
            New product
          </Button>
        }
      />

      <FilterBar
        search={{
          value: filters.search,
          onChange: (value) => setFilter('search', value),
          placeholder: 'Search SKU, name or barcode…',
        }}
        selects={[
          {
            label: 'Type',
            value: filters.product_type,
            onChange: (value) => setFilter('product_type', value),
            options: PRODUCT_TYPES,
          },
        ]}
        onReset={reset}
      />

      <DataTable
        columns={columns}
        rows={query.data?.data}
        rowKey={(row) => row.id}
        isLoading={query.isLoading}
        error={query.error}
        onRetry={query.refetch}
        onRowClick={(row) => navigate(`/inventory/products/${row.id}`)}
        pagination={query.data?.pagination}
        onPageChange={setPage}
        sort={sort}
        onSortChange={setSort}
        emptyTitle="No products yet"
        emptyMessage="Add the goods and services you sell."
        emptyAction={<Button onClick={() => setCreating(true)}>New product</Button>}
      />

      {creating && <ProductDialog product={null} onClose={() => setCreating(false)} />}
    </div>
  );
}

export function ProductDetail() {
  const { id } = useParams<{ id: string }>();
  const query = products.useOne(id);
  const [editing, setEditing] = useState(false);

  if (query.isLoading) return <DetailSkeleton />;
  if (query.error) return <ErrorState error={query.error} onRetry={query.refetch} />;
  if (!query.data) return null;

  const product = query.data;
  const lowStock = product.stock_levels.filter((level) => level.needs_reorder);

  return (
    <div className="space-y-6">
      <PageHeader
        title={product.name}
        description={product.sku}
        backTo="/inventory/products"
        backLabel="Back to products"
        badge={
          product.is_active ? <Badge tone="success">Active</Badge> : <Badge tone="muted">Inactive</Badge>
        }
        actions={
          <Button variant="outline" onClick={() => setEditing(true)}>
            <Pencil className="mr-1 h-4 w-4" />
            Edit
          </Button>
        }
      />

      {lowStock.length > 0 && (
        <div className="flex items-start gap-3 rounded-md border border-amber-200 bg-amber-50 p-4">
          <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-amber-500" />
          <p className="text-sm text-amber-900">
            <span className="font-medium">Reorder needed.</span> This product is at or below its
            reorder level in {lowStock.length} warehouse{lowStock.length > 1 ? 's' : ''}.
          </p>
        </div>
      )}

      <SummaryGrid
        items={[
          { label: 'On hand', value: formatNumber(product.total_on_hand) },
          // The gap between these two is stock promised to confirmed orders.
          // Both figures have always been here; until reservations existed they
          // were always the same number.
          {
            label: 'Reserved',
            value: formatNumber(product.total_on_hand - product.total_available),
          },
          { label: 'Available', value: formatNumber(product.total_available) },
          { label: 'Cost price', value: formatMoney(product.cost_price) },
          // Derived from what was actually paid, not typed in — which is why it
          // sits beside the standing cost price rather than replacing it. The
          // two disagreeing is normal and informative.
          { label: 'Average cost', value: formatMoney(product.average_cost) },
          { label: 'Sale price', value: formatMoney(product.sale_price) },
        ]}
      />

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Stock by warehouse</CardTitle>
        </CardHeader>
        <CardContent>
          {product.stock_levels.length === 0 ? (
            <EmptyState
              title="No stock recorded"
              message="Record a stock movement to bring this product into a warehouse."
            />
          ) : (
            <Table>
              <TableHeader>
                <TableRow className="hover:bg-transparent">
                  <TableHead>Warehouse</TableHead>
                  <TableHead className="text-right">On hand</TableHead>
                  <TableHead className="text-right">Reserved</TableHead>
                  <TableHead className="text-right">Available</TableHead>
                  <TableHead className="text-right">Reorder at</TableHead>
                  <TableHead />
                </TableRow>
              </TableHeader>
              <TableBody>
                {product.stock_levels.map((level) => (
                  <TableRow key={level.id}>
                    <TableCell className="font-mono text-xs">{level.warehouse_id.slice(0, 8)}</TableCell>
                    <TableCell className="text-right tabular-nums">{level.quantity}</TableCell>
                    <TableCell className="text-right tabular-nums">{level.reserved_quantity}</TableCell>
                    <TableCell className="text-right font-medium tabular-nums">
                      {level.available}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {level.reorder_level ?? '—'}
                    </TableCell>
                    <TableCell className="text-right">
                      {level.needs_reorder && <Badge tone="warning">Reorder</Badge>}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {product.description && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Description</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="whitespace-pre-wrap text-sm text-slate-600">{product.description}</p>
          </CardContent>
        </Card>
      )}

      {editing && <ProductDialog product={product} onClose={() => setEditing(false)} />}
    </div>
  );
}

function ProductDialog({ product, onClose }: { product: Product | null; onClose: () => void }) {
  const isEdit = Boolean(product);
  const { data: categories } = useCategories();
  const create = products.useCreate({ successMessage: 'Product created', onSuccess: onClose });
  const update = products.useUpdate({ successMessage: 'Product updated', onSuccess: onClose });

  const form = useForm<ProductFormValues>({
    resolver: zodResolver(productSchema),
    defaultValues: {
      sku: product?.sku ?? '',
      name: product?.name ?? '',
      description: product?.description ?? '',
      product_type: (product?.product_type as ProductFormValues['product_type']) ?? 'product',
      category_id: product?.category_id ?? '',
      unit_of_measure: product?.unit_of_measure ?? 'piece',
      cost_price: product?.cost_price ? Number(product.cost_price) : undefined,
      sale_price: product?.sale_price ? Number(product.sale_price) : undefined,
      tax_rate: product?.tax_rate ? Number(product.tax_rate) : undefined,
      barcode: product?.barcode ?? '',
      weight: product?.weight ? Number(product.weight) : undefined,
      dimensions: product?.dimensions ?? '',
    },
  });

  const busy = create.isPending || update.isPending;
  const { errors } = form.formState;

  const categoryOptions = (categories ?? []).map((category) => ({
    value: category.id,
    label: category.name,
  }));

  const onSubmit = form.handleSubmit((values) => {
    const body = toPayload(values, ['cost_price', 'sale_price', 'tax_rate', 'weight']);
    if (product) {
      // SKU is the product's identity and the API does not accept a change.
      delete body.sku;
      update.mutate({ id: product.id, body });
    } else {
      create.mutate(body);
    }
  });

  return (
    <Dialog
      open
      onClose={onClose}
      title={isEdit ? 'Edit product' : 'New product'}
      className="max-w-2xl"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={busy}>
            {busy ? 'Saving…' : isEdit ? 'Save changes' : 'Create product'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <FormGrid>
          <Field label="SKU" required error={errors.sku?.message} hint={isEdit ? 'SKU cannot be changed' : undefined}>
            <Input disabled={isEdit} {...form.register('sku')} />
          </Field>
          <Field label="Name" required error={errors.name?.message}>
            <Input {...form.register('name')} />
          </Field>
          <Field label="Type" required error={errors.product_type?.message}>
            <Select options={PRODUCT_TYPES} {...form.register('product_type')} />
          </Field>
          <Field label="Category" error={errors.category_id?.message}>
            <Select options={categoryOptions} placeholder="Uncategorised" {...form.register('category_id')} />
          </Field>
          <Field label="Unit of measure" error={errors.unit_of_measure?.message}>
            <Input placeholder="piece" {...form.register('unit_of_measure')} />
          </Field>
          <Field label="Barcode" error={errors.barcode?.message}>
            <Input {...form.register('barcode')} />
          </Field>
          <Field label="Cost price" error={errors.cost_price?.message}>
            <Input type="number" step="0.01" min={0} {...form.register('cost_price')} />
          </Field>
          <Field label="Sale price" error={errors.sale_price?.message}>
            <Input type="number" step="0.01" min={0} {...form.register('sale_price')} />
          </Field>
          <Field label="Tax rate %" error={errors.tax_rate?.message}>
            <Input type="number" step="0.01" min={0} {...form.register('tax_rate')} />
          </Field>
          <Field label="Weight" error={errors.weight?.message}>
            <Input type="number" step="0.001" min={0} {...form.register('weight')} />
          </Field>
        </FormGrid>
        <Field label="Description" error={errors.description?.message}>
          <Textarea {...form.register('description')} />
        </Field>
      </form>
    </Dialog>
  );
}
