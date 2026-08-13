import { useState } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { Pencil, Plus, Trash2 } from 'lucide-react';
import { PageHeader } from '@/components/common/PageHeader';
import { DataTable, type Column } from '@/components/common/DataTable';
import { FilterBar } from '@/components/common/Filters';
import { Field, FormGrid } from '@/components/common/Field';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { Textarea } from '@/components/ui/Textarea';
import { Dialog, ConfirmDialog } from '@/components/ui/Dialog';
import { StatusBadge } from '@/components/ui/Badge';
import { CurrencyField } from '@/components/common/CurrencyField';
import { useListParams } from '@/hooks/useListParams';
import { activities, companies, contacts, opportunities, useCompanyOptions } from '@/hooks/useCrm';
import { formatDate, formatMoney } from '@/lib/utils';
import { toPayload } from '@/schemas/common';
import {
  activitySchema,
  companySchema,
  contactSchema,
  opportunitySchema,
  type ActivityForm,
  type CompanyForm,
  type ContactForm,
  type OpportunityForm,
} from '@/schemas';
import {
  ACTIVITY_TYPES,
  COMPANY_TYPES,
  CONTACT_STATUSES,
  OPPORTUNITY_STAGES,
  type Activity,
  type Company,
  type Contact,
  type Opportunity,
} from '@/types';

type Tab = 'contacts' | 'companies' | 'opportunities' | 'activities';

const TABS: Array<{ id: Tab; label: string }> = [
  { id: 'contacts', label: 'Contacts' },
  { id: 'companies', label: 'Companies' },
  { id: 'opportunities', label: 'Opportunities' },
  { id: 'activities', label: 'Activities' },
];

export function CRMPage() {
  const [tab, setTab] = useState<Tab>('contacts');

  return (
    <div className="space-y-6">
      <PageHeader title="CRM" description="Contacts, companies, pipeline and activity history" />

      <div className="flex gap-1 overflow-x-auto border-b">
        {TABS.map((item) => (
          <button
            key={item.id}
            onClick={() => setTab(item.id)}
            className={`whitespace-nowrap border-b-2 px-4 py-2 text-sm font-medium transition-colors ${
              tab === item.id
                ? 'border-primary text-primary'
                : 'border-transparent text-slate-500 hover:text-slate-800'
            }`}
          >
            {item.label}
          </button>
        ))}
      </div>

      {tab === 'contacts' && <ContactsTab />}
      {tab === 'companies' && <CompaniesTab />}
      {tab === 'opportunities' && <OpportunitiesTab />}
      {tab === 'activities' && <ActivitiesTab />}
    </div>
  );
}

// ---------------------------------------------------------------- contacts

function ContactsTab() {
  const { params, page, setPage, sort, setSort, filters, setFilter, reset } = useListParams({
    search: '',
    status: '',
  });
  const query = contacts.useList(params);
  const [editing, setEditing] = useState<Contact | null>(null);
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState<Contact | null>(null);
  const remove = contacts.useRemove({ successMessage: 'Contact deleted' });

  const columns: Column<Contact>[] = [
    {
      key: 'last_name',
      header: 'Name',
      sortable: true,
      render: (row) => (
        <div>
          <p className="font-medium text-slate-900">
            {row.first_name} {row.last_name}
          </p>
          {row.job_title && <p className="text-xs text-slate-500">{row.job_title}</p>}
        </div>
      ),
    },
    { key: 'email', header: 'Email', render: (row) => row.email ?? '—' },
    { key: 'phone', header: 'Phone', render: (row) => row.phone ?? '—' },
    { key: 'status', header: 'Status', render: (row) => <StatusBadge status={row.status} /> },
    {
      key: 'created_at',
      header: 'Created',
      sortable: true,
      render: (row) => formatDate(row.created_at),
    },
    {
      key: 'actions',
      header: '',
      align: 'right',
      render: (row) => <RowActions onEdit={() => setEditing(row)} onDelete={() => setDeleting(row)} />,
    },
  ];

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <FilterBar
          search={{
            value: filters.search,
            onChange: (value) => setFilter('search', value),
            placeholder: 'Search contacts…',
          }}
          selects={[
            {
              label: 'Status',
              value: filters.status,
              onChange: (value) => setFilter('status', value),
              options: CONTACT_STATUSES,
            },
          ]}
          onReset={reset}
        />
        <Button onClick={() => setCreating(true)}>
          <Plus className="mr-1 h-4 w-4" />
          New contact
        </Button>
      </div>

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
        emptyTitle="No contacts yet"
        emptyMessage="Add the people you do business with."
        emptyAction={<Button onClick={() => setCreating(true)}>New contact</Button>}
      />

      {(creating || editing) && (
        <ContactDialog
          contact={editing}
          onClose={() => {
            setCreating(false);
            setEditing(null);
          }}
        />
      )}

      <ConfirmDialog
        open={Boolean(deleting)}
        onClose={() => setDeleting(null)}
        onConfirm={() => {
          if (deleting) remove.mutate(deleting.id, { onSuccess: () => setDeleting(null) });
        }}
        title="Delete contact"
        message={`Delete ${deleting?.first_name} ${deleting?.last_name}? This cannot be undone.`}
        confirmLabel="Delete"
        busy={remove.isPending}
      />
      <span className="sr-only">{page}</span>
    </div>
  );
}

function ContactDialog({ contact, onClose }: { contact: Contact | null; onClose: () => void }) {
  const isEdit = Boolean(contact);
  const { options: companyOptions } = useCompanyOptions();
  const create = contacts.useCreate({ successMessage: 'Contact created', onSuccess: onClose });
  const update = contacts.useUpdate({ successMessage: 'Contact updated', onSuccess: onClose });

  const form = useForm<ContactForm>({
    resolver: zodResolver(contactSchema),
    defaultValues: {
      first_name: contact?.first_name ?? '',
      last_name: contact?.last_name ?? '',
      email: contact?.email ?? '',
      phone: contact?.phone ?? '',
      mobile: contact?.mobile ?? '',
      job_title: contact?.job_title ?? '',
      company_id: contact?.company_id ?? '',
      status: (contact?.status as ContactForm['status']) ?? 'lead',
      city: contact?.city ?? '',
      country: contact?.country ?? '',
      address: contact?.address ?? '',
      notes: contact?.notes ?? '',
    },
  });

  const busy = create.isPending || update.isPending;
  const { errors } = form.formState;

  const onSubmit = form.handleSubmit((values) => {
    const body = toPayload(values);
    if (contact) update.mutate({ id: contact.id, body });
    else create.mutate(body);
  });

  return (
    <Dialog
      open
      onClose={onClose}
      title={isEdit ? 'Edit contact' : 'New contact'}
      className="max-w-2xl"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={busy}>
            {busy ? 'Saving…' : isEdit ? 'Save changes' : 'Create contact'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <FormGrid>
          <Field label="First name" required error={errors.first_name?.message}>
            <Input {...form.register('first_name')} />
          </Field>
          <Field label="Last name" required error={errors.last_name?.message}>
            <Input {...form.register('last_name')} />
          </Field>
          <Field label="Email" error={errors.email?.message}>
            <Input type="email" {...form.register('email')} />
          </Field>
          <Field label="Phone" error={errors.phone?.message}>
            <Input {...form.register('phone')} />
          </Field>
          <Field label="Job title" error={errors.job_title?.message}>
            <Input {...form.register('job_title')} />
          </Field>
          <Field label="Company" error={errors.company_id?.message}>
            <Select options={companyOptions} placeholder="No company" {...form.register('company_id')} />
          </Field>
          <Field label="Status" required error={errors.status?.message}>
            <Select options={CONTACT_STATUSES} {...form.register('status')} />
          </Field>
          <Field label="City" error={errors.city?.message}>
            <Input {...form.register('city')} />
          </Field>
        </FormGrid>
        <Field label="Notes" error={errors.notes?.message}>
          <Textarea {...form.register('notes')} />
        </Field>
      </form>
    </Dialog>
  );
}

// --------------------------------------------------------------- companies

function CompaniesTab() {
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams({
    search: '',
    company_type: '',
  });
  const query = companies.useList(params);
  const [editing, setEditing] = useState<Company | null>(null);
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState<Company | null>(null);
  const remove = companies.useRemove({ successMessage: 'Company deleted' });

  const columns: Column<Company>[] = [
    {
      key: 'name',
      header: 'Company',
      sortable: true,
      render: (row) => (
        <div>
          <p className="font-medium text-slate-900">{row.name}</p>
          {row.industry && <p className="text-xs text-slate-500">{row.industry}</p>}
        </div>
      ),
    },
    { key: 'company_type', header: 'Type', render: (row) => <StatusBadge status={row.company_type} /> },
    { key: 'email', header: 'Email', render: (row) => row.email ?? '—' },
    { key: 'country', header: 'Country', render: (row) => row.country ?? '—' },
    { key: 'status', header: 'Status', render: (row) => <StatusBadge status={row.status} /> },
    {
      key: 'actions',
      header: '',
      align: 'right',
      render: (row) => <RowActions onEdit={() => setEditing(row)} onDelete={() => setDeleting(row)} />,
    },
  ];

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <FilterBar
          search={{
            value: filters.search,
            onChange: (value) => setFilter('search', value),
            placeholder: 'Search companies…',
          }}
          selects={[
            {
              label: 'Type',
              value: filters.company_type,
              onChange: (value) => setFilter('company_type', value),
              options: COMPANY_TYPES,
            },
          ]}
          onReset={reset}
        />
        <Button onClick={() => setCreating(true)}>
          <Plus className="mr-1 h-4 w-4" />
          New company
        </Button>
      </div>

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
        emptyTitle="No companies yet"
        emptyMessage="Customers, suppliers and partners live here."
        emptyAction={<Button onClick={() => setCreating(true)}>New company</Button>}
      />

      {(creating || editing) && (
        <CompanyDialog
          company={editing}
          onClose={() => {
            setCreating(false);
            setEditing(null);
          }}
        />
      )}

      <ConfirmDialog
        open={Boolean(deleting)}
        onClose={() => setDeleting(null)}
        onConfirm={() => {
          if (deleting) remove.mutate(deleting.id, { onSuccess: () => setDeleting(null) });
        }}
        title="Delete company"
        message={`Delete ${deleting?.name}? Contacts linked to it will be unlinked.`}
        confirmLabel="Delete"
        busy={remove.isPending}
      />
    </div>
  );
}

function CompanyDialog({ company, onClose }: { company: Company | null; onClose: () => void }) {
  const isEdit = Boolean(company);
  const create = companies.useCreate({ successMessage: 'Company created', onSuccess: onClose });
  const update = companies.useUpdate({ successMessage: 'Company updated', onSuccess: onClose });

  const form = useForm<CompanyForm>({
    resolver: zodResolver(companySchema),
    defaultValues: {
      name: company?.name ?? '',
      legal_name: company?.legal_name ?? '',
      tax_id: company?.tax_id ?? '',
      company_type: (company?.company_type as CompanyForm['company_type']) ?? 'prospect',
      email: company?.email ?? '',
      phone: company?.phone ?? '',
      website: company?.website ?? '',
      industry: company?.industry ?? '',
      city: company?.city ?? '',
      country: company?.country ?? '',
      address: company?.address ?? '',
    },
  });

  const busy = create.isPending || update.isPending;
  const { errors } = form.formState;

  const onSubmit = form.handleSubmit((values) => {
    const body = toPayload(values);
    if (company) update.mutate({ id: company.id, body });
    else create.mutate(body);
  });

  return (
    <Dialog
      open
      onClose={onClose}
      title={isEdit ? 'Edit company' : 'New company'}
      className="max-w-2xl"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={busy}>
            {busy ? 'Saving…' : isEdit ? 'Save changes' : 'Create company'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <FormGrid>
          <Field label="Name" required error={errors.name?.message}>
            <Input {...form.register('name')} />
          </Field>
          <Field label="Type" required error={errors.company_type?.message}>
            <Select options={COMPANY_TYPES} {...form.register('company_type')} />
          </Field>
          <Field label="Legal name" error={errors.legal_name?.message}>
            <Input {...form.register('legal_name')} />
          </Field>
          <Field label="Tax ID" error={errors.tax_id?.message}>
            <Input {...form.register('tax_id')} />
          </Field>
          <Field label="Email" error={errors.email?.message}>
            <Input type="email" {...form.register('email')} />
          </Field>
          <Field label="Phone" error={errors.phone?.message}>
            <Input {...form.register('phone')} />
          </Field>
          <Field label="Website" error={errors.website?.message} hint="Include https://">
            <Input {...form.register('website')} />
          </Field>
          <Field label="Industry" error={errors.industry?.message}>
            <Input {...form.register('industry')} />
          </Field>
          <Field label="City" error={errors.city?.message}>
            <Input {...form.register('city')} />
          </Field>
          <Field label="Country" error={errors.country?.message}>
            <Input {...form.register('country')} />
          </Field>
        </FormGrid>
      </form>
    </Dialog>
  );
}

// ----------------------------------------------------------- opportunities

function OpportunitiesTab() {
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams({
    search: '',
    stage: '',
  });
  const query = opportunities.useList(params);
  const [editing, setEditing] = useState<Opportunity | null>(null);
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState<Opportunity | null>(null);
  const remove = opportunities.useRemove({ successMessage: 'Opportunity deleted' });

  const columns: Column<Opportunity>[] = [
    { key: 'title', header: 'Opportunity', render: (row) => <span className="font-medium">{row.title}</span> },
    { key: 'stage', header: 'Stage', render: (row) => <StatusBadge status={row.stage} /> },
    {
      key: 'value',
      header: 'Value',
      sortable: true,
      align: 'right',
      render: (row) => formatMoney(row.value, row.currency),
    },
    {
      key: 'probability',
      header: 'Probability',
      align: 'right',
      render: (row) => (row.probability === null ? '—' : `${row.probability}%`),
    },
    {
      key: 'expected_close_date',
      header: 'Expected close',
      sortable: true,
      render: (row) => formatDate(row.expected_close_date),
    },
    {
      key: 'actions',
      header: '',
      align: 'right',
      render: (row) => <RowActions onEdit={() => setEditing(row)} onDelete={() => setDeleting(row)} />,
    },
  ];

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <FilterBar
          search={{
            value: filters.search,
            onChange: (value) => setFilter('search', value),
            placeholder: 'Search opportunities…',
          }}
          selects={[
            {
              label: 'Stage',
              value: filters.stage,
              onChange: (value) => setFilter('stage', value),
              options: OPPORTUNITY_STAGES,
            },
          ]}
          onReset={reset}
        />
        <Button onClick={() => setCreating(true)}>
          <Plus className="mr-1 h-4 w-4" />
          New opportunity
        </Button>
      </div>

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
        emptyTitle="No opportunities yet"
        emptyMessage="Track deals as they move through the pipeline."
        emptyAction={<Button onClick={() => setCreating(true)}>New opportunity</Button>}
      />

      {(creating || editing) && (
        <OpportunityDialog
          opportunity={editing}
          onClose={() => {
            setCreating(false);
            setEditing(null);
          }}
        />
      )}

      <ConfirmDialog
        open={Boolean(deleting)}
        onClose={() => setDeleting(null)}
        onConfirm={() => {
          if (deleting) remove.mutate(deleting.id, { onSuccess: () => setDeleting(null) });
        }}
        title="Delete opportunity"
        message={`Delete "${deleting?.title}"?`}
        confirmLabel="Delete"
        busy={remove.isPending}
      />
    </div>
  );
}

function OpportunityDialog({
  opportunity,
  onClose,
}: {
  opportunity: Opportunity | null;
  onClose: () => void;
}) {
  const isEdit = Boolean(opportunity);
  const { options: companyOptions } = useCompanyOptions();
  const create = opportunities.useCreate({ successMessage: 'Opportunity created', onSuccess: onClose });
  const update = opportunities.useUpdate({ successMessage: 'Opportunity updated', onSuccess: onClose });

  const form = useForm<OpportunityForm>({
    resolver: zodResolver(opportunitySchema),
    defaultValues: {
      title: opportunity?.title ?? '',
      company_id: opportunity?.company_id ?? '',
      contact_id: opportunity?.contact_id ?? '',
      stage: (opportunity?.stage as OpportunityForm['stage']) ?? 'prospecting',
      value: opportunity?.value ? Number(opportunity.value) : undefined,
      currency: opportunity?.currency ?? 'USD',
      probability: opportunity?.probability ?? undefined,
      expected_close_date: opportunity?.expected_close_date ?? '',
      source: opportunity?.source ?? '',
      description: opportunity?.description ?? '',
    },
  });

  const busy = create.isPending || update.isPending;
  const { errors } = form.formState;

  const onSubmit = form.handleSubmit((values) => {
    // `value` is money: send it as a fixed-scale string.
    const body = toPayload(values, ['value']);
    if (opportunity) update.mutate({ id: opportunity.id, body });
    else create.mutate(body);
  });

  return (
    <Dialog
      open
      onClose={onClose}
      title={isEdit ? 'Edit opportunity' : 'New opportunity'}
      className="max-w-2xl"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={busy}>
            {busy ? 'Saving…' : isEdit ? 'Save changes' : 'Create opportunity'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <Field label="Title" required error={errors.title?.message}>
          <Input {...form.register('title')} />
        </Field>
        <FormGrid>
          <Field label="Customer" required error={errors.company_id?.message}>
            <Select options={companyOptions} placeholder="Select a company" {...form.register('company_id')} />
          </Field>
          <Field label="Stage" required error={errors.stage?.message}>
            <Select options={OPPORTUNITY_STAGES} {...form.register('stage')} />
          </Field>
          <Field label="Value" error={errors.value?.message}>
            <Input type="number" step="0.01" min={0} {...form.register('value')} />
          </Field>
          <CurrencyField {...form.register('currency')} />
          <Field label="Probability %" error={errors.probability?.message}>
            <Input type="number" min={0} max={100} {...form.register('probability')} />
          </Field>
          <Field label="Expected close" error={errors.expected_close_date?.message}>
            <Input type="date" {...form.register('expected_close_date')} />
          </Field>
        </FormGrid>
        <Field label="Description" error={errors.description?.message}>
          <Textarea {...form.register('description')} />
        </Field>
      </form>
    </Dialog>
  );
}

// -------------------------------------------------------------- activities

function ActivitiesTab() {
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams({
    status: '',
  });
  const query = activities.useList(params);
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState<Activity | null>(null);
  const remove = activities.useRemove({ successMessage: 'Activity deleted' });
  const update = activities.useUpdate({ successMessage: 'Activity updated' });

  const columns: Column<Activity>[] = [
    { key: 'subject', header: 'Subject', render: (row) => <span className="font-medium">{row.subject}</span> },
    { key: 'activity_type', header: 'Type', render: (row) => <StatusBadge status={row.activity_type} /> },
    { key: 'status', header: 'Status', render: (row) => <StatusBadge status={row.status} /> },
    {
      key: 'created_at',
      header: 'Created',
      sortable: true,
      render: (row) => formatDate(row.created_at),
    },
    {
      key: 'actions',
      header: '',
      align: 'right',
      render: (row) => (
        <div className="flex justify-end gap-2">
          {row.status === 'scheduled' && (
            <Button
              size="sm"
              variant="outline"
              onClick={() => update.mutate({ id: row.id, body: { status: 'completed' } })}
            >
              Complete
            </Button>
          )}
          <RowActions onDelete={() => setDeleting(row)} />
        </div>
      ),
    },
  ];

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <FilterBar
          selects={[
            {
              label: 'Status',
              value: filters.status,
              onChange: (value) => setFilter('status', value),
              options: ['scheduled', 'completed', 'cancelled'],
            },
          ]}
          onReset={reset}
        />
        <Button onClick={() => setCreating(true)}>
          <Plus className="mr-1 h-4 w-4" />
          Log activity
        </Button>
      </div>

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
        emptyTitle="No activities logged"
        emptyMessage="Calls, meetings and notes appear here."
      />

      {creating && <ActivityDialog onClose={() => setCreating(false)} />}

      <ConfirmDialog
        open={Boolean(deleting)}
        onClose={() => setDeleting(null)}
        onConfirm={() => {
          if (deleting) remove.mutate(deleting.id, { onSuccess: () => setDeleting(null) });
        }}
        title="Delete activity"
        message={`Delete "${deleting?.subject}"?`}
        confirmLabel="Delete"
        busy={remove.isPending}
      />
    </div>
  );
}

function ActivityDialog({ onClose }: { onClose: () => void }) {
  const create = activities.useCreate({ successMessage: 'Activity logged', onSuccess: onClose });

  const form = useForm<ActivityForm>({
    resolver: zodResolver(activitySchema),
    defaultValues: { activity_type: 'call', subject: '', description: '' },
  });

  const { errors } = form.formState;
  const onSubmit = form.handleSubmit((values) => create.mutate(toPayload(values)));

  return (
    <Dialog
      open
      onClose={onClose}
      title="Log activity"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={create.isPending}>
            {create.isPending ? 'Saving…' : 'Log activity'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <Field label="Type" required error={errors.activity_type?.message}>
          <Select options={ACTIVITY_TYPES} {...form.register('activity_type')} />
        </Field>
        <Field label="Subject" required error={errors.subject?.message}>
          <Input {...form.register('subject')} />
        </Field>
        <Field label="Description" error={errors.description?.message}>
          <Textarea {...form.register('description')} />
        </Field>
      </form>
    </Dialog>
  );
}

/** Edit/delete buttons shared by every CRM table row. */
function RowActions({ onEdit, onDelete }: { onEdit?: () => void; onDelete?: () => void }) {
  return (
    <div className="flex justify-end gap-1">
      {onEdit && (
        <button
          onClick={(event) => {
            event.stopPropagation();
            onEdit();
          }}
          className="rounded p-1.5 text-slate-400 transition-colors hover:bg-slate-100 hover:text-slate-700"
          aria-label="Edit"
        >
          <Pencil className="h-4 w-4" />
        </button>
      )}
      {onDelete && (
        <button
          onClick={(event) => {
            event.stopPropagation();
            onDelete();
          }}
          className="rounded p-1.5 text-slate-400 transition-colors hover:bg-red-50 hover:text-red-600"
          aria-label="Delete"
        >
          <Trash2 className="h-4 w-4" />
        </button>
      )}
    </div>
  );
}

