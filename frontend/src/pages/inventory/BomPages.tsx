import { useNavigate, useParams } from 'react-router-dom';
import { useFieldArray, useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { Plus, Trash2 } from 'lucide-react';
import { PageHeader } from '@/components/common/PageHeader';
import { DataTable, type Column } from '@/components/common/DataTable';
import { Field, FormGrid } from '@/components/common/Field';
import { DetailSkeleton } from '@/components/ui/Skeleton';
import { ErrorState } from '@/components/common/States';
import { Button } from '@/components/ui/Button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { Badge } from '@/components/ui/Badge';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/Table';
import { useListParams } from '@/hooks/useListParams';
import { boms, useProductOptions } from '@/hooks/useInventory';
import { bomSchema, type BomForm as BomFormValues } from '@/schemas';
import type { BillOfMaterials } from '@/types';

export function BomList() {
  const navigate = useNavigate();
  const { params, setPage, sort, setSort } = useListParams({});
  const query = boms.useList(params);
  const { products } = useProductOptions();

  const nameFor = (productId: string) =>
    products.find((product) => product.id === productId)?.name ?? productId.slice(0, 8);

  const columns: Column<BillOfMaterials>[] = [
    {
      key: 'product_id',
      header: 'Product',
      render: (row) => <span className="font-medium text-slate-900">{nameFor(row.product_id)}</span>,
    },
    { key: 'version', header: 'Version', render: (row) => row.version },
    {
      key: 'quantity_to_produce',
      header: 'Produces',
      align: 'right',
      render: (row) => <span className="tabular-nums">{row.quantity_to_produce}</span>,
    },
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
        title="Bills of materials"
        description="What each manufactured product is built from"
        actions={
          <Button onClick={() => navigate('/inventory/boms/new')}>
            <Plus className="mr-1 h-4 w-4" />
            New BOM
          </Button>
        }
      />

      <DataTable
        columns={columns}
        rows={query.data?.data}
        rowKey={(row) => row.id}
        isLoading={query.isLoading}
        error={query.error}
        onRetry={query.refetch}
        onRowClick={(row) => navigate(`/inventory/boms/${row.id}`)}
        pagination={query.data?.pagination}
        onPageChange={setPage}
        sort={sort}
        onSortChange={setSort}
        emptyTitle="No bills of materials"
        emptyMessage="Define the components that go into a manufactured product."
        emptyAction={<Button onClick={() => navigate('/inventory/boms/new')}>New BOM</Button>}
      />
    </div>
  );
}

export function BomDetail() {
  const { id } = useParams<{ id: string }>();
  const query = boms.useOne(id);
  const { products } = useProductOptions();

  if (query.isLoading) return <DetailSkeleton />;
  if (query.error) return <ErrorState error={query.error} onRetry={query.refetch} />;
  if (!query.data) return null;

  const bom = query.data;
  const nameFor = (productId: string) =>
    products.find((product) => product.id === productId)?.name ?? productId.slice(0, 8);

  return (
    <div className="space-y-6">
      <PageHeader
        title={`${nameFor(bom.product_id)} — v${bom.version}`}
        description={`Produces ${bom.quantity_to_produce} unit(s) per run`}
        backTo="/inventory/boms"
        backLabel="Back to BOMs"
        badge={bom.is_active ? <Badge tone="success">Active</Badge> : <Badge tone="muted">Inactive</Badge>}
      />

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Components</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow className="hover:bg-transparent">
                <TableHead>Component</TableHead>
                <TableHead className="text-right">Quantity</TableHead>
                <TableHead>Unit</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {bom.lines.map((line) => (
                <TableRow key={line.id}>
                  <TableCell className="font-medium">{nameFor(line.component_id)}</TableCell>
                  <TableCell className="text-right tabular-nums">{line.quantity_required}</TableCell>
                  <TableCell className="text-slate-500">{line.unit_of_measure}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}

export function BomForm() {
  const navigate = useNavigate();
  const { options: productOptions } = useProductOptions();
  const create = boms.useCreate({
    successMessage: 'Bill of materials created',
    onSuccess: (bom) => navigate(`/inventory/boms/${bom.id}`),
  });

  const form = useForm<BomFormValues>({
    resolver: zodResolver(bomSchema),
    defaultValues: {
      product_id: '',
      version: '1.0',
      quantity_to_produce: 1,
      lines: [{ component_id: '', quantity_required: 1, unit_of_measure: 'piece' }],
    },
  });

  const { fields, append, remove } = useFieldArray({ control: form.control, name: 'lines' });
  const { errors } = form.formState;
  // The self-reference rule attaches to the array itself.
  const linesError = (errors.lines as { message?: string } | undefined)?.message;

  const onSubmit = form.handleSubmit((values) => create.mutate(values));

  return (
    <form onSubmit={onSubmit} className="space-y-6" noValidate>
      <PageHeader
        title="New bill of materials"
        backTo="/inventory/boms"
        backLabel="Back to BOMs"
        actions={
          <>
            <Button type="button" variant="outline" onClick={() => navigate('/inventory/boms')}>
              Cancel
            </Button>
            <Button type="submit" disabled={create.isPending}>
              {create.isPending ? 'Saving…' : 'Create BOM'}
            </Button>
          </>
        }
      />

      <Card>
        <CardContent className="pt-6">
          <FormGrid>
            <Field label="Product" required error={errors.product_id?.message}>
              <Select
                options={productOptions}
                placeholder="What is being built"
                {...form.register('product_id')}
              />
            </Field>
            <Field label="Version" error={errors.version?.message}>
              <Input {...form.register('version')} />
            </Field>
            <Field label="Quantity produced" required error={errors.quantity_to_produce?.message}>
              <Input type="number" min={1} {...form.register('quantity_to_produce')} />
            </Field>
          </FormGrid>
        </CardContent>
      </Card>

      <Card>
        <CardContent className="space-y-3 pt-6">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-slate-900">Components</h3>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() =>
                append({ component_id: '', quantity_required: 1, unit_of_measure: 'piece' })
              }
            >
              <Plus className="mr-1 h-4 w-4" />
              Add component
            </Button>
          </div>

          {linesError && (
            <p className="text-xs font-medium text-red-600" role="alert">
              {linesError}
            </p>
          )}

          <div className="space-y-3">
            {fields.map((field, index) => {
              const rowError = (errors.lines ?? [])[index] as
                | Record<string, { message?: string }>
                | undefined;

              return (
                <div key={field.id} className="flex items-start gap-3">
                  <div className="flex-1">
                    <Select
                      options={productOptions}
                      placeholder="Select a component"
                      {...form.register(`lines.${index}.component_id`)}
                    />
                    {rowError?.component_id?.message && (
                      <p className="mt-1 text-xs text-red-600">{rowError.component_id.message}</p>
                    )}
                  </div>
                  <div className="w-28">
                    <Input
                      type="number"
                      min={1}
                      className="text-right"
                      {...form.register(`lines.${index}.quantity_required`)}
                    />
                    {rowError?.quantity_required?.message && (
                      <p className="mt-1 text-xs text-red-600">
                        {rowError.quantity_required.message}
                      </p>
                    )}
                  </div>
                  <div className="w-32">
                    <Input placeholder="piece" {...form.register(`lines.${index}.unit_of_measure`)} />
                  </div>
                  <button
                    type="button"
                    onClick={() => remove(index)}
                    className="mt-2 rounded p-1.5 text-slate-400 hover:bg-red-50 hover:text-red-600"
                    aria-label={`Remove component ${index + 1}`}
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              );
            })}
          </div>
        </CardContent>
      </Card>
    </form>
  );
}

