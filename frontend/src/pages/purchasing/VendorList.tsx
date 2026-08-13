import { useState } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { Pencil, Plus } from 'lucide-react';
import { PageHeader } from '@/components/common/PageHeader';
import { DataTable, type Column } from '@/components/common/DataTable';
import { FilterBar } from '@/components/common/Filters';
import { Field, FormGrid } from '@/components/common/Field';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Textarea } from '@/components/ui/Textarea';
import { Dialog } from '@/components/ui/Dialog';
import { StatusBadge } from '@/components/ui/Badge';
import { CurrencyField } from '@/components/common/CurrencyField';
import { useListParams } from '@/hooks/useListParams';
import { vendors } from '@/hooks/usePurchasing';
import { toPayload } from '@/schemas/common';
import { vendorSchema, type VendorForm } from '@/schemas';
import type { Vendor } from '@/types';

export function VendorList() {
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams({
    search: '',
    status: '',
  });
  const query = vendors.useList(params);
  const [editing, setEditing] = useState<Vendor | null>(null);
  const [creating, setCreating] = useState(false);

  const columns: Column<Vendor>[] = [
    {
      key: 'name',
      header: 'Vendor',
      sortable: true,
      render: (row) => (
        <div>
          <p className="font-medium text-slate-900">{row.name}</p>
          {row.legal_name && <p className="text-xs text-slate-500">{row.legal_name}</p>}
        </div>
      ),
    },
    { key: 'email', header: 'Email', render: (row) => row.email ?? '—' },
    { key: 'country', header: 'Country', render: (row) => row.country ?? '—' },
    { key: 'payment_terms', header: 'Terms', render: (row) => row.payment_terms ?? '—' },
    { key: 'currency', header: 'Currency', render: (row) => row.currency },
    { key: 'status', header: 'Status', render: (row) => <StatusBadge status={row.status} /> },
    {
      key: 'actions',
      header: '',
      align: 'right',
      render: (row) => (
        <button
          onClick={() => setEditing(row)}
          className="rounded p-1.5 text-slate-400 transition-colors hover:bg-slate-100 hover:text-slate-700"
          aria-label="Edit vendor"
        >
          <Pencil className="h-4 w-4" />
        </button>
      ),
    },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Vendors"
        description="Suppliers you raise purchase orders against"
        actions={
          <Button onClick={() => setCreating(true)}>
            <Plus className="mr-1 h-4 w-4" />
            New vendor
          </Button>
        }
      />

      <FilterBar
        search={{
          value: filters.search,
          onChange: (value) => setFilter('search', value),
          placeholder: 'Search vendors…',
        }}
        selects={[
          {
            label: 'Status',
            value: filters.status,
            onChange: (value) => setFilter('status', value),
            options: ['active', 'inactive'],
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
        emptyTitle="No vendors yet"
        emptyMessage="Add a supplier before raising a purchase order."
        emptyAction={<Button onClick={() => setCreating(true)}>New vendor</Button>}
      />

      {(creating || editing) && (
        <VendorDialog
          vendor={editing}
          onClose={() => {
            setCreating(false);
            setEditing(null);
          }}
        />
      )}
    </div>
  );
}

function VendorDialog({ vendor, onClose }: { vendor: Vendor | null; onClose: () => void }) {
  const isEdit = Boolean(vendor);
  const create = vendors.useCreate({ successMessage: 'Vendor created', onSuccess: onClose });
  const update = vendors.useUpdate({ successMessage: 'Vendor updated', onSuccess: onClose });

  const form = useForm<VendorForm>({
    resolver: zodResolver(vendorSchema),
    defaultValues: {
      name: vendor?.name ?? '',
      legal_name: vendor?.legal_name ?? '',
      tax_id: vendor?.tax_id ?? '',
      email: vendor?.email ?? '',
      phone: vendor?.phone ?? '',
      address: vendor?.address ?? '',
      city: vendor?.city ?? '',
      country: vendor?.country ?? '',
      payment_terms: vendor?.payment_terms ?? '',
      currency: vendor?.currency ?? 'USD',
    },
  });

  const busy = create.isPending || update.isPending;
  const { errors } = form.formState;

  const onSubmit = form.handleSubmit((values) => {
    const body = toPayload(values);
    if (vendor) update.mutate({ id: vendor.id, body });
    else create.mutate(body);
  });

  return (
    <Dialog
      open
      onClose={onClose}
      title={isEdit ? 'Edit vendor' : 'New vendor'}
      className="max-w-2xl"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={busy}>
            {busy ? 'Saving…' : isEdit ? 'Save changes' : 'Create vendor'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <FormGrid>
          <Field label="Name" required error={errors.name?.message}>
            <Input {...form.register('name')} />
          </Field>
          <Field label="Legal name" error={errors.legal_name?.message}>
            <Input {...form.register('legal_name')} />
          </Field>
          <Field label="Email" error={errors.email?.message}>
            <Input type="email" {...form.register('email')} />
          </Field>
          <Field label="Phone" error={errors.phone?.message}>
            <Input {...form.register('phone')} />
          </Field>
          <Field label="Tax ID" error={errors.tax_id?.message}>
            <Input {...form.register('tax_id')} />
          </Field>
          <Field label="Payment terms" error={errors.payment_terms?.message} hint="e.g. Net 30">
            <Input {...form.register('payment_terms')} />
          </Field>
          <Field label="City" error={errors.city?.message}>
            <Input {...form.register('city')} />
          </Field>
          <Field label="Country" error={errors.country?.message}>
            <Input {...form.register('country')} />
          </Field>
          <CurrencyField {...form.register('currency')} />
        </FormGrid>
        <Field label="Address" error={errors.address?.message}>
          <Textarea {...form.register('address')} />
        </Field>
      </form>
    </Dialog>
  );
}
