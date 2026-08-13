import { useState } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { ChevronRight, Plus, RefreshCw } from 'lucide-react';
import { PageHeader } from '@/components/common/PageHeader';
import { DataTable, type Column } from '@/components/common/DataTable';
import { FilterBar } from '@/components/common/Filters';
import { Field, FormGrid } from '@/components/common/Field';
import { EmptyState, ErrorState } from '@/components/common/States';
import { TableSkeleton } from '@/components/ui/Skeleton';
import { Button } from '@/components/ui/Button';
import { Card, CardContent } from '@/components/ui/Card';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { Dialog, ConfirmDialog } from '@/components/ui/Dialog';
import { Badge, StatusBadge } from '@/components/ui/Badge';
import { CurrencyField } from '@/components/common/CurrencyField';
import { useListParams } from '@/hooks/useListParams';
import {
  accounts,
  bankAccounts,
  ledgerEntries,
  taxRates,
  useAccountOptions,
  useAccountTree,
  useRecalculateBalances,
} from '@/hooks/useAccounting';
import { formatDate, formatMoney, humanize } from '@/lib/utils';
import { toPayload } from '@/schemas/common';
import {
  accountSchema,
  bankAccountSchema,
  ledgerEntrySchema,
  taxRateSchema,
  type AccountForm,
  type BankAccountForm,
  type LedgerEntryForm,
  type TaxRateForm,
} from '@/schemas';
import {
  ACCOUNT_TYPES,
  type AccountNode,
  type BankAccount,
  type GeneralLedgerEntry,
  type TaxRate,
} from '@/types';

export function ChartOfAccounts() {
  const tree = useAccountTree();
  const recalculate = useRecalculateBalances();
  const [creating, setCreating] = useState(false);

  return (
    <div className="space-y-6">
      <PageHeader
        title="Chart of accounts"
        description="The account hierarchy every journal entry posts against"
        actions={
          <>
            <Button
              variant="outline"
              onClick={() => recalculate.mutate()}
              disabled={recalculate.isPending}
            >
              <RefreshCw className="mr-1 h-4 w-4" />
              {recalculate.isPending ? 'Recalculating…' : 'Recalculate balances'}
            </Button>
            <Button onClick={() => setCreating(true)}>
              <Plus className="mr-1 h-4 w-4" />
              New account
            </Button>
          </>
        }
      />

      <Card>
        <CardContent className="pt-6">
          {tree.isLoading ? (
            <TableSkeleton columns={3} />
          ) : tree.error ? (
            <ErrorState error={tree.error} onRetry={tree.refetch} />
          ) : !tree.data?.length ? (
            <EmptyState
              title="No accounts yet"
              message="Build a chart of accounts before posting journal entries."
              action={<Button onClick={() => setCreating(true)}>New account</Button>}
            />
          ) : (
            <div className="space-y-1">
              {tree.data.map((node) => (
                <AccountRow key={node.id} node={node} depth={0} />
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {creating && <AccountDialog onClose={() => setCreating(false)} />}
    </div>
  );
}

function AccountRow({ node, depth }: { node: AccountNode; depth: number }) {
  const [open, setOpen] = useState(true);
  const hasChildren = node.children.length > 0;

  return (
    <div>
      <div
        className="flex items-center gap-2 rounded px-2 py-2 hover:bg-slate-50"
        style={{ paddingLeft: `${depth * 1.5 + 0.5}rem` }}
      >
        {hasChildren ? (
          <button
            onClick={() => setOpen(!open)}
            className="rounded p-0.5 text-slate-400 hover:bg-slate-200"
            aria-label={open ? 'Collapse' : 'Expand'}
          >
            <ChevronRight
              className={`h-4 w-4 transition-transform ${open ? 'rotate-90' : ''}`}
            />
          </button>
        ) : (
          <span className="w-5" />
        )}

        <span className="w-20 font-mono text-xs text-slate-500">{node.account_code}</span>
        <span className="flex-1 text-sm font-medium text-slate-900">{node.account_name}</span>
        <StatusBadge status={node.account_type} />
        {node.is_bank_account && <Badge tone="info">Bank</Badge>}
        <span className="w-32 text-right text-sm font-medium tabular-nums">
          {formatMoney(node.current_balance, node.currency)}
        </span>
      </div>

      {open &&
        node.children.map((child) => (
          <AccountRow key={child.id} node={child} depth={depth + 1} />
        ))}
    </div>
  );
}

function AccountDialog({ onClose }: { onClose: () => void }) {
  const create = accounts.useCreate({ successMessage: 'Account created', onSuccess: onClose });
  const { options: parentOptions } = useAccountOptions();

  const form = useForm<AccountForm>({
    resolver: zodResolver(accountSchema),
    defaultValues: {
      account_code: '',
      account_name: '',
      account_type: 'asset',
      parent_id: '',
      is_bank_account: false,
      opening_balance: 0,
    },
  });

  const { errors } = form.formState;
  const onSubmit = form.handleSubmit((values) =>
    create.mutate(toPayload(values, ['opening_balance']))
  );

  return (
    <Dialog
      open
      onClose={onClose}
      title="New account"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={create.isPending}>
            {create.isPending ? 'Saving…' : 'Create account'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <FormGrid>
          <Field label="Account code" required error={errors.account_code?.message}>
            <Input placeholder="1000" {...form.register('account_code')} />
          </Field>
          <Field label="Account name" required error={errors.account_name?.message}>
            <Input placeholder="Cash at bank" {...form.register('account_name')} />
          </Field>
          <Field label="Type" required error={errors.account_type?.message}>
            <Select options={ACCOUNT_TYPES} {...form.register('account_type')} />
          </Field>
          <Field label="Parent account" error={errors.parent_id?.message}>
            <Select options={parentOptions} placeholder="Top level" {...form.register('parent_id')} />
          </Field>
          <CurrencyField {...form.register('currency')} />
          <Field label="Opening balance" error={errors.opening_balance?.message}>
            <Input type="number" step="0.01" {...form.register('opening_balance')} />
          </Field>
        </FormGrid>
        <label className="flex items-center gap-2 text-sm text-slate-700">
          <input type="checkbox" className="h-4 w-4 rounded border-input" {...form.register('is_bank_account')} />
          This is a bank account
        </label>
      </form>
    </Dialog>
  );
}

export function GeneralLedger() {
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams(
    { search: '' },
    { defaultSort: '-entry_date' }
  );
  const query = ledgerEntries.useList(params);
  const { accounts: accountList } = useAccountOptions();
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState<GeneralLedgerEntry | null>(null);
  const remove = ledgerEntries.useRemove({ successMessage: 'Entry reversed' });

  const labelFor = (accountId: string) => {
    const account = accountList.find((item) => item.id === accountId);
    return account ? `${account.account_code} ${account.account_name}` : accountId.slice(0, 8);
  };

  const columns: Column<GeneralLedgerEntry>[] = [
    { key: 'entry_date', header: 'Date', sortable: true, render: (row) => formatDate(row.entry_date) },
    {
      key: 'description',
      header: 'Description',
      render: (row) => <span className="font-medium">{row.description}</span>,
    },
    { key: 'debit_account_id', header: 'Debit', render: (row) => labelFor(row.debit_account_id) },
    { key: 'credit_account_id', header: 'Credit', render: (row) => labelFor(row.credit_account_id) },
    {
      key: 'amount',
      header: 'Amount',
      sortable: true,
      align: 'right',
      render: (row) => (
        <span className="font-medium tabular-nums">{formatMoney(row.amount, row.currency)}</span>
      ),
    },
    {
      key: 'actions',
      header: '',
      align: 'right',
      render: (row) => (
        <Button size="sm" variant="ghost" onClick={() => setDeleting(row)}>
          Reverse
        </Button>
      ),
    },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="General ledger"
        description="Double-entry journal — every line debits one account and credits another"
        actions={
          <Button onClick={() => setCreating(true)}>
            <Plus className="mr-1 h-4 w-4" />
            New journal entry
          </Button>
        }
      />

      <FilterBar
        search={{
          value: filters.search,
          onChange: (value) => setFilter('search', value),
          placeholder: 'Search descriptions…',
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
        emptyTitle="No journal entries"
        emptyMessage="Post an entry to start the ledger."
        emptyAction={<Button onClick={() => setCreating(true)}>New journal entry</Button>}
      />

      {creating && <LedgerEntryDialog onClose={() => setCreating(false)} />}

      <ConfirmDialog
        open={Boolean(deleting)}
        onClose={() => setDeleting(null)}
        onConfirm={() => {
          if (deleting) remove.mutate(deleting.id, { onSuccess: () => setDeleting(null) });
        }}
        title="Reverse journal entry"
        message="Both account balances will be unwound by this entry's amount."
        confirmLabel="Reverse entry"
        busy={remove.isPending}
      />
    </div>
  );
}

function LedgerEntryDialog({ onClose }: { onClose: () => void }) {
  const create = ledgerEntries.useCreate({ successMessage: 'Journal entry posted', onSuccess: onClose });
  const { options: accountOptions } = useAccountOptions();

  const form = useForm<LedgerEntryForm>({
    resolver: zodResolver(ledgerEntrySchema),
    defaultValues: {
      entry_date: new Date().toISOString().slice(0, 10),
      description: '',
      debit_account_id: '',
      credit_account_id: '',
      amount: 0,
    },
  });

  const { errors } = form.formState;
  const onSubmit = form.handleSubmit((values) => create.mutate(toPayload(values, ['amount'])));

  return (
    <Dialog
      open
      onClose={onClose}
      title="New journal entry"
      description="One amount, debited from one account and credited to another."
      className="max-w-xl"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={create.isPending}>
            {create.isPending ? 'Posting…' : 'Post entry'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <Field label="Description" required error={errors.description?.message}>
          <Input placeholder="What this entry records" {...form.register('description')} />
        </Field>
        <FormGrid>
          <Field label="Debit account" required error={errors.debit_account_id?.message}>
            <Select
              options={accountOptions}
              placeholder="Account debited"
              {...form.register('debit_account_id')}
            />
          </Field>
          <Field label="Credit account" required error={errors.credit_account_id?.message}>
            <Select
              options={accountOptions}
              placeholder="Account credited"
              {...form.register('credit_account_id')}
            />
          </Field>
          <Field label="Amount" required error={errors.amount?.message}>
            <Input type="number" step="0.01" min={0.01} {...form.register('amount')} />
          </Field>
          <Field label="Entry date" required error={errors.entry_date?.message}>
            <Input type="date" {...form.register('entry_date')} />
          </Field>
        </FormGrid>
      </form>
    </Dialog>
  );
}

export function BankAccountList() {
  const { params, setPage } = useListParams({});
  const query = bankAccounts.useList(params);
  const { accounts: accountList } = useAccountOptions();
  const [creating, setCreating] = useState(false);

  const columns: Column<BankAccount>[] = [
    {
      key: 'bank_name',
      header: 'Bank',
      render: (row) => <span className="font-medium text-slate-900">{row.bank_name}</span>,
    },
    {
      key: 'account_number',
      header: 'Account number',
      render: (row) => <span className="font-mono text-xs">{row.account_number}</span>,
    },
    { key: 'iban', header: 'IBAN', render: (row) => row.iban ?? '—' },
    {
      key: 'account_id',
      header: 'Ledger account',
      render: (row) => {
        const account = accountList.find((item) => item.id === row.account_id);
        return account ? `${account.account_code} ${account.account_name}` : '—';
      },
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
        title="Bank accounts"
        description="Bank details linked to their ledger accounts"
        actions={
          <Button onClick={() => setCreating(true)}>
            <Plus className="mr-1 h-4 w-4" />
            New bank account
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
        pagination={query.data?.pagination}
        onPageChange={setPage}
        emptyTitle="No bank accounts"
        emptyMessage="Link a bank account to the ledger account it settles against."
        emptyAction={<Button onClick={() => setCreating(true)}>New bank account</Button>}
      />

      {creating && <BankAccountDialog onClose={() => setCreating(false)} />}
    </div>
  );
}

function BankAccountDialog({ onClose }: { onClose: () => void }) {
  const create = bankAccounts.useCreate({ successMessage: 'Bank account created', onSuccess: onClose });
  // Only accounts flagged as bank accounts make sense here.
  const { options: accountOptions } = useAccountOptions({ account_type: 'asset' });

  const form = useForm<BankAccountForm>({
    resolver: zodResolver(bankAccountSchema),
    defaultValues: { account_id: '', bank_name: '', account_number: '', iban: '', swift: '', branch: '' },
  });

  const { errors } = form.formState;
  const onSubmit = form.handleSubmit((values) => create.mutate(toPayload(values)));

  return (
    <Dialog
      open
      onClose={onClose}
      title="New bank account"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={create.isPending}>
            {create.isPending ? 'Saving…' : 'Create bank account'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <Field label="Ledger account" required error={errors.account_id?.message}>
          <Select
            options={accountOptions}
            placeholder="Which asset account this settles to"
            {...form.register('account_id')}
          />
        </Field>
        <FormGrid>
          <Field label="Bank name" required error={errors.bank_name?.message}>
            <Input {...form.register('bank_name')} />
          </Field>
          <Field label="Account number" required error={errors.account_number?.message}>
            <Input {...form.register('account_number')} />
          </Field>
          <Field label="IBAN" error={errors.iban?.message}>
            <Input {...form.register('iban')} />
          </Field>
          <Field label="SWIFT" error={errors.swift?.message}>
            <Input {...form.register('swift')} />
          </Field>
          <Field label="Branch" error={errors.branch?.message}>
            <Input {...form.register('branch')} />
          </Field>
        </FormGrid>
      </form>
    </Dialog>
  );
}

export function TaxRateList() {
  const { params, setPage } = useListParams({});
  const query = taxRates.useList(params);
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState<TaxRate | null>(null);
  const remove = taxRates.useRemove({ successMessage: 'Tax rate deleted' });

  const columns: Column<TaxRate>[] = [
    {
      key: 'name',
      header: 'Name',
      render: (row) => <span className="font-medium text-slate-900">{row.name}</span>,
    },
    {
      key: 'rate',
      header: 'Rate',
      align: 'right',
      render: (row) => (
        <span className="font-medium tabular-nums">{Number(row.rate).toFixed(2)}%</span>
      ),
    },
    { key: 'tax_type', header: 'Type', render: (row) => humanize(row.tax_type) },
    { key: 'country', header: 'Country', render: (row) => row.country ?? '—' },
    {
      key: 'actions',
      header: '',
      align: 'right',
      render: (row) => (
        <Button size="sm" variant="ghost" onClick={() => setDeleting(row)}>
          Delete
        </Button>
      ),
    },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Tax rates"
        description="A whole percentage — 20 means 20%, the same as a tax rate on a document line"
        actions={
          <Button onClick={() => setCreating(true)}>
            <Plus className="mr-1 h-4 w-4" />
            New tax rate
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
        pagination={query.data?.pagination}
        onPageChange={setPage}
        emptyTitle="No tax rates"
        emptyMessage="Configure the rates your invoices apply."
        emptyAction={<Button onClick={() => setCreating(true)}>New tax rate</Button>}
      />

      {creating && <TaxRateDialog onClose={() => setCreating(false)} />}

      <ConfirmDialog
        open={Boolean(deleting)}
        onClose={() => setDeleting(null)}
        onConfirm={() => {
          if (deleting) remove.mutate(deleting.id, { onSuccess: () => setDeleting(null) });
        }}
        title="Delete tax rate"
        message={`Delete "${deleting?.name}"?`}
        confirmLabel="Delete"
        busy={remove.isPending}
      />
    </div>
  );
}

function TaxRateDialog({ onClose }: { onClose: () => void }) {
  const create = taxRates.useCreate({ successMessage: 'Tax rate created', onSuccess: onClose });

  const form = useForm<TaxRateForm>({
    resolver: zodResolver(taxRateSchema),
    defaultValues: { name: '', rate: 20, tax_type: 'vat', country: '' },
  });

  const { errors } = form.formState;
  const rate = form.watch('rate');
  const onSubmit = form.handleSubmit((values) => create.mutate(toPayload(values)));

  return (
    <Dialog
      open
      onClose={onClose}
      title="New tax rate"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={create.isPending}>
            {create.isPending ? 'Saving…' : 'Create tax rate'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <FormGrid>
          <Field label="Name" required error={errors.name?.message}>
            <Input placeholder="Standard VAT" {...form.register('name')} />
          </Field>
          <Field
            label="Rate"
            required
            error={errors.rate?.message}
            hint={`Fraction — currently ${(Number(rate) * 100 || 0).toFixed(2)}%`}
          >
            <Input type="number" step="0.0001" min={0} max={1} {...form.register('rate')} />
          </Field>
          <Field label="Tax type" required error={errors.tax_type?.message}>
            <Input placeholder="vat" {...form.register('tax_type')} />
          </Field>
          <Field label="Country" error={errors.country?.message}>
            <Input {...form.register('country')} />
          </Field>
        </FormGrid>
      </form>
    </Dialog>
  );
}
