import { useState } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { ArrowDownToLine, Plus } from 'lucide-react';
import { PageHeader } from '@/components/common/PageHeader';
import { DataTable, type Column } from '@/components/common/DataTable';
import { FilterBar } from '@/components/common/Filters';
import { Field, FormGrid } from '@/components/common/Field';
import { Button } from '@/components/ui/Button';
import { Card, CardContent } from '@/components/ui/Card';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { Textarea } from '@/components/ui/Textarea';
import { Dialog } from '@/components/ui/Dialog';
import { Badge, StatusBadge } from '@/components/ui/Badge';
import { useListParams } from '@/hooks/useListParams';
import {
  movements,
  useLowStock,
  useProductOptions,
  useRecordMovement,
  useStockValuation,
  useWarehouseOptions,
  warehouses,
} from '@/hooks/useInventory';
import { formatDateTime, formatMoney } from '@/lib/utils';
import { toPayload } from '@/schemas/common';
import { movementSchema, warehouseSchema, type MovementForm, type WarehouseForm } from '@/schemas';
import { MOVEMENT_TYPES, type StockLevelView, type StockMovement, type Warehouse } from '@/types';

export function WarehouseList() {
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams({
    search: '',
  });
  const query = warehouses.useList(params);
  const [creating, setCreating] = useState(false);
  const valuation = useStockValuation();

  const columns: Column<Warehouse>[] = [
    {
      key: 'code',
      header: 'Code',
      sortable: true,
      render: (row) => <span className="font-mono text-xs font-medium">{row.code}</span>,
    },
    {
      key: 'name',
      header: 'Warehouse',
      sortable: true,
      render: (row) => <span className="font-medium text-slate-900">{row.name}</span>,
    },
    { key: 'city', header: 'City', render: (row) => row.city ?? '—' },
    { key: 'country', header: 'Country', render: (row) => row.country ?? '—' },
    {
      key: 'is_active',
      header: 'Status',
      render: (row) =>
        row.is_active ? <Badge tone="success">Active</Badge> : <Badge tone="muted">Inactive</Badge>,
    },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Warehouses"
        description={
          valuation.data
            ? `Total stock at cost: ${formatMoney(valuation.data.total_value)}`
            : 'Where stock is held'
        }
        actions={
          <Button onClick={() => setCreating(true)}>
            <Plus className="mr-1 h-4 w-4" />
            New warehouse
          </Button>
        }
      />

      <FilterBar
        search={{
          value: filters.search,
          onChange: (value) => setFilter('search', value),
          placeholder: 'Search warehouses…',
        }}
        onReset={reset}
      />

      <DataTable
        columns={columns}
        rows={query.data?.data}
        rowKey={(row) => row.id}
        isLoading={query.isLoading}
        error={query.error}
        onRetry={query.refetch}
        pagination={query.data?.pagination}
        onPageChange={setPage}
        sort={sort}
        onSortChange={setSort}
        emptyTitle="No warehouses"
        emptyMessage="Stock has to live somewhere — add your first warehouse."
        emptyAction={<Button onClick={() => setCreating(true)}>New warehouse</Button>}
      />

      {creating && <WarehouseDialog onClose={() => setCreating(false)} />}
    </div>
  );
}

function WarehouseDialog({ onClose }: { onClose: () => void }) {
  const create = warehouses.useCreate({ successMessage: 'Warehouse created', onSuccess: onClose });

  const form = useForm<WarehouseForm>({
    resolver: zodResolver(warehouseSchema),
    defaultValues: { code: '', name: '', address: '', city: '', country: '' },
  });

  const { errors } = form.formState;
  const onSubmit = form.handleSubmit((values) => create.mutate(toPayload(values)));

  return (
    <Dialog
      open
      onClose={onClose}
      title="New warehouse"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={create.isPending}>
            {create.isPending ? 'Saving…' : 'Create warehouse'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <FormGrid>
          <Field label="Code" required error={errors.code?.message} hint="Short, unique, e.g. WH1">
            <Input {...form.register('code')} />
          </Field>
          <Field label="Name" required error={errors.name?.message}>
            <Input {...form.register('name')} />
          </Field>
          <Field label="City" error={errors.city?.message}>
            <Input {...form.register('city')} />
          </Field>
          <Field label="Country" error={errors.country?.message}>
            <Input {...form.register('country')} />
          </Field>
        </FormGrid>
        <Field label="Address" error={errors.address?.message}>
          <Textarea {...form.register('address')} />
        </Field>
      </form>
    </Dialog>
  );
}

export function MovementList() {
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams({
    movement_type: '',
  });
  const query = movements.useList(params);
  const [recording, setRecording] = useState(false);
  const lowStock = useLowStock({ per_page: 5 });

  const columns: Column<StockMovement>[] = [
    {
      key: 'created_at',
      header: 'When',
      sortable: true,
      render: (row) => formatDateTime(row.created_at),
    },
    {
      key: 'movement_type',
      header: 'Type',
      render: (row) => <StatusBadge status={row.movement_type} />,
    },
    {
      key: 'quantity',
      header: 'Quantity',
      sortable: true,
      align: 'right',
      render: (row) => (
        <span
          className={`font-medium tabular-nums ${
            row.quantity < 0 || row.movement_type === 'out' ? 'text-red-600' : 'text-green-600'
          }`}
        >
          {row.movement_type === 'out' ? '−' : row.quantity < 0 ? '' : '+'}
          {Math.abs(row.quantity)}
        </span>
      ),
    },
    {
      key: 'unit_cost',
      header: 'Unit cost',
      align: 'right',
      render: (row) => <span className="tabular-nums">{formatMoney(row.unit_cost)}</span>,
    },
    {
      key: 'reference_type',
      header: 'Source',
      render: (row) => (row.reference_type ? <StatusBadge status={row.reference_type} /> : 'Manual'),
    },
    { key: 'notes', header: 'Notes', render: (row) => row.notes ?? '—' },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Stock movements"
        description="The full audit trail — stock levels only ever change through this ledger"
        actions={
          <Button onClick={() => setRecording(true)}>
            <ArrowDownToLine className="mr-1 h-4 w-4" />
            Record movement
          </Button>
        }
      />

      {lowStock.data && lowStock.data.pagination.total > 0 && (
        <Card className="border-amber-200 bg-amber-50">
          <CardContent className="pt-6">
            <p className="text-sm font-medium text-amber-900">
              {lowStock.data.pagination.total} product/warehouse pair
              {lowStock.data.pagination.total > 1 ? 's are' : ' is'} at or below the reorder level.
            </p>
            <ul className="mt-2 space-y-1 text-sm text-amber-800">
              {lowStock.data.data.map((level: StockLevelView) => (
                <li key={level.id} className="font-mono text-xs">
                  {level.product_id.slice(0, 8)} — {level.available} available (reorder at{' '}
                  {level.reorder_level})
                </li>
              ))}
            </ul>
          </CardContent>
        </Card>
      )}

      <FilterBar
        selects={[
          {
            label: 'Type',
            value: filters.movement_type,
            onChange: (value) => setFilter('movement_type', value),
            options: MOVEMENT_TYPES,
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
        pagination={query.data?.pagination}
        onPageChange={setPage}
        sort={sort}
        onSortChange={setSort}
        emptyTitle="No stock movements"
        emptyMessage="Receiving goods or recording a movement will populate this log."
        emptyAction={<Button onClick={() => setRecording(true)}>Record movement</Button>}
      />

      {recording && <MovementDialog onClose={() => setRecording(false)} />}
    </div>
  );
}

function MovementDialog({ onClose }: { onClose: () => void }) {
  const record = useRecordMovement();
  const { options: productOptions } = useProductOptions();
  const { options: warehouseOptions } = useWarehouseOptions();

  const form = useForm<MovementForm>({
    resolver: zodResolver(movementSchema),
    defaultValues: {
      product_id: '',
      warehouse_id: '',
      to_warehouse_id: '',
      movement_type: 'in',
      quantity: 1,
      notes: '',
    },
  });

  const movementType = form.watch('movement_type');
  const isTransfer = movementType === 'transfer';
  const isAdjustment = movementType === 'adjustment';
  const { errors } = form.formState;

  const onSubmit = form.handleSubmit((values) =>
    record.mutate(toPayload(values, ['unit_cost']), { onSuccess: onClose })
  );

  return (
    <Dialog
      open
      onClose={onClose}
      title="Record stock movement"
      className="max-w-xl"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={record.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={record.isPending}>
            {record.isPending ? 'Recording…' : 'Record movement'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <FormGrid>
          <Field label="Product" required error={errors.product_id?.message}>
            <Select options={productOptions} placeholder="Select a product" {...form.register('product_id')} />
          </Field>
          <Field label="Movement type" required error={errors.movement_type?.message}>
            <Select options={MOVEMENT_TYPES} {...form.register('movement_type')} />
          </Field>
          <Field
            label={isTransfer ? 'From warehouse' : 'Warehouse'}
            required
            error={errors.warehouse_id?.message}
          >
            <Select
              options={warehouseOptions}
              placeholder="Select a warehouse"
              {...form.register('warehouse_id')}
            />
          </Field>
          {isTransfer && (
            <Field label="To warehouse" required error={errors.to_warehouse_id?.message}>
              <Select
                options={warehouseOptions}
                placeholder="Destination"
                {...form.register('to_warehouse_id')}
              />
            </Field>
          )}
          <Field
            label="Quantity"
            required
            error={errors.quantity?.message}
            hint={isAdjustment ? 'Negative values write stock down' : undefined}
          >
            <Input type="number" step={1} {...form.register('quantity')} />
          </Field>
          <Field label="Unit cost" error={errors.unit_cost?.message} hint="Defaults to the product cost">
            <Input type="number" step="0.01" min={0} {...form.register('unit_cost')} />
          </Field>
        </FormGrid>
        <Field label="Notes" error={errors.notes?.message}>
          <Textarea {...form.register('notes')} />
        </Field>
      </form>
    </Dialog>
  );
}
