import { useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { Controller, useFieldArray, useForm } from 'react-hook-form';
import { Check, Plus, Trash2, X } from 'lucide-react';
import { zodResolver } from '@hookform/resolvers/zod';
import { PageHeader } from '@/components/common/PageHeader';
import { DataTable, type Column } from '@/components/common/DataTable';
import { FilterBar } from '@/components/common/Filters';
import { SummaryGrid } from '@/components/common/DocumentView';
import { Field, FormGrid } from '@/components/common/Field';
import { DetailSkeleton } from '@/components/ui/Skeleton';
import { ErrorState } from '@/components/common/States';
import { Button } from '@/components/ui/Button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { Textarea } from '@/components/ui/Textarea';
import { Dialog } from '@/components/ui/Dialog';
import { StatusBadge } from '@/components/ui/Badge';
import { CurrencyField } from '@/components/common/CurrencyField';
import { FileUpload } from '@/components/common/FileUpload';
import { ReceiptLink } from '@/components/common/ReceiptLink';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/Table';
import { useListParams } from '@/hooks/useListParams';
import {
  employees,
  expenseReports,
  leaveRequests,
  useDecideLeave,
  useEmployeeOptions,
  useUserOptions,
  useExpenseStatus,
} from '@/hooks/useHr';
import { useAuthStore } from '@/store/authStore';
import { formatDate, formatMoney, toDateInput } from '@/lib/utils';
import { toPayload } from '@/schemas/common';
import {
  employeeSchema,
  expenseReportSchema,
  leaveRequestSchema,
  type EmployeeForm,
  type ExpenseReportForm,
  type LeaveRequestForm,
} from '@/schemas';
import {
  EMPLOYEE_STATUSES,
  EXPENSE_STATUSES,
  LEAVE_STATUSES,
  LEAVE_TYPES,
  type Employee,
  type ExpenseReport,
  type LeaveRequest,
} from '@/types';

export function EmployeeList() {
  const navigate = useNavigate();
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams({
    search: '',
    status: '',
  });
  const query = employees.useList(params);
  const [creating, setCreating] = useState(false);
  const canManage = useAuthStore((state) => state.hasRole('hr', 'manager'));

  const columns: Column<Employee>[] = [
    {
      key: 'employee_number',
      header: 'Number',
      sortable: true,
      render: (row) => <span className="font-mono text-xs">{row.employee_number}</span>,
    },
    {
      key: 'last_name',
      header: 'Employee',
      sortable: true,
      render: (row) => (
        <div>
          <p className="font-medium text-slate-900">
            {row.first_name} {row.last_name}
          </p>
          <p className="text-xs text-slate-500">{row.email}</p>
        </div>
      ),
    },
    { key: 'department', header: 'Department', render: (row) => row.department ?? '—' },
    { key: 'job_title', header: 'Job title', render: (row) => row.job_title ?? '—' },
    { key: 'hire_date', header: 'Hired', sortable: true, render: (row) => formatDate(row.hire_date) },
    { key: 'status', header: 'Status', render: (row) => <StatusBadge status={row.status} /> },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Employees"
        description="The people directory behind leave, expenses and timesheets"
        actions={
          canManage && (
            <Button onClick={() => setCreating(true)}>
              <Plus className="mr-1 h-4 w-4" />
              New employee
            </Button>
          )
        }
      />

      <FilterBar
        search={{
          value: filters.search,
          onChange: (value) => setFilter('search', value),
          placeholder: 'Search employees…',
        }}
        selects={[
          {
            label: 'Status',
            value: filters.status,
            onChange: (value) => setFilter('status', value),
            options: EMPLOYEE_STATUSES,
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
        onRowClick={(row) => navigate(`/hr/employees/${row.id}`)}
        pagination={query.data?.pagination}
        onPageChange={setPage}
        sort={sort}
        onSortChange={setSort}
        emptyTitle="No employees"
        emptyMessage="Add employees so they can book leave and claim expenses."
        emptyAction={canManage && <Button onClick={() => setCreating(true)}>New employee</Button>}
      />

      {creating && <EmployeeDialog onClose={() => setCreating(false)} />}
    </div>
  );
}

export function EmployeeDetail() {
  const { id } = useParams<{ id: string }>();
  const query = employees.useOne(id);

  if (query.isLoading) return <DetailSkeleton />;
  if (query.error) return <ErrorState error={query.error} onRetry={query.refetch} />;
  if (!query.data) return null;

  const employee = query.data;
  const balance = employee.leave_balance;

  return (
    <div className="space-y-6">
      <PageHeader
        title={`${employee.first_name} ${employee.last_name}`}
        description={`${employee.employee_number} · ${employee.email}`}
        backTo="/hr/employees"
        backLabel="Back to employees"
        badge={<StatusBadge status={employee.status} />}
      />

      <SummaryGrid
        items={[
          { label: 'Department', value: employee.department ?? '—' },
          { label: 'Job title', value: employee.job_title ?? '—' },
          { label: 'Hired', value: formatDate(employee.hire_date) },
          { label: 'Salary', value: formatMoney(employee.salary, employee.currency) },
        ]}
      />

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Annual leave — {balance.year}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="grid gap-4 sm:grid-cols-3">
            <div>
              <p className="text-xs uppercase text-slate-500">Entitlement</p>
              <p className="text-2xl font-bold">{balance.entitlement}</p>
            </div>
            <div>
              <p className="text-xs uppercase text-slate-500">Taken</p>
              <p className="text-2xl font-bold">{balance.taken}</p>
            </div>
            <div>
              <p className="text-xs uppercase text-slate-500">Remaining</p>
              <p
                className={`text-2xl font-bold ${
                  balance.remaining <= 0 ? 'text-red-600' : 'text-green-600'
                }`}
              >
                {balance.remaining}
              </p>
            </div>
          </div>

          <div className="h-2 overflow-hidden rounded-full bg-slate-100">
            <div
              className="h-full rounded-full bg-primary transition-all"
              style={{
                width: `${Math.min(
                  100,
                  balance.entitlement > 0 ? (balance.taken / balance.entitlement) * 100 : 0
                )}%`,
              }}
            />
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

function EmployeeDialog({ onClose }: { onClose: () => void }) {
  const create = employees.useCreate({ successMessage: 'Employee created', onSuccess: onClose });
  const { options: managerOptions } = useEmployeeOptions();
  const { options: userOptions } = useUserOptions();

  const form = useForm<EmployeeForm>({
    resolver: zodResolver(employeeSchema),
    defaultValues: {
      first_name: '',
      last_name: '',
      email: '',
      hire_date: toDateInput(),
      annual_leave_entitlement: 25,
    },
  });

  const { errors } = form.formState;
  const onSubmit = form.handleSubmit((values) => create.mutate(toPayload(values, ['salary'])));

  return (
    <Dialog
      open
      onClose={onClose}
      title="New employee"
      className="max-w-2xl"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={create.isPending}>
            {create.isPending ? 'Saving…' : 'Create employee'}
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
          <Field label="Email" required error={errors.email?.message}>
            <Input type="email" {...form.register('email')} />
          </Field>
          <Field label="Phone" error={errors.phone?.message}>
            <Input {...form.register('phone')} />
          </Field>
          <Field
            label="Employee number"
            error={errors.employee_number?.message}
            hint="Generated if left blank"
          >
            <Input {...form.register('employee_number')} />
          </Field>
          <Field label="Hire date" required error={errors.hire_date?.message}>
            <Input type="date" {...form.register('hire_date')} />
          </Field>
          <Field label="Department" error={errors.department?.message}>
            <Input {...form.register('department')} />
          </Field>
          <Field label="Job title" error={errors.job_title?.message}>
            <Input {...form.register('job_title')} />
          </Field>
          <Field label="Manager" error={errors.manager_id?.message}>
            <Select options={managerOptions} placeholder="No manager" {...form.register('manager_id')} />
          </Field>
          <Field
            label="Login"
            error={errors.user_id?.message}
            hint="Links this person to their account. Without it they cannot see their own leave or expenses."
          >
            <Select options={userOptions} placeholder="No login" {...form.register('user_id')} />
          </Field>
          <Field label="Salary" error={errors.salary?.message}>
            <Input type="number" step="0.01" min={0} {...form.register('salary')} />
          </Field>
          <Field
            label="Annual leave days"
            required
            error={errors.annual_leave_entitlement?.message}
          >
            <Input type="number" min={0} max={365} {...form.register('annual_leave_entitlement')} />
          </Field>
          <CurrencyField {...form.register('currency')} />
        </FormGrid>
      </form>
    </Dialog>
  );
}

export function LeaveRequestList() {
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams(
    { status: '', leave_type: '' },
    { defaultSort: '-start_date' }
  );
  const query = leaveRequests.useList(params);
  const decide = useDecideLeave();
  const [creating, setCreating] = useState(false);
  const canDecide = useAuthStore((state) => state.hasRole('hr', 'manager'));
  const { employees: employeeList } = useEmployeeOptions();

  const nameFor = (employeeId: string) => {
    const employee = employeeList.find((item) => item.id === employeeId);
    return employee ? `${employee.first_name} ${employee.last_name}` : employeeId.slice(0, 8);
  };

  const columns: Column<LeaveRequest>[] = [
    {
      key: 'employee_id',
      header: 'Employee',
      render: (row) => <span className="font-medium">{nameFor(row.employee_id)}</span>,
    },
    { key: 'leave_type', header: 'Type', render: (row) => <StatusBadge status={row.leave_type} /> },
    {
      key: 'start_date',
      header: 'Dates',
      sortable: true,
      render: (row) => (
        <span>
          {formatDate(row.start_date)} → {formatDate(row.end_date)}
        </span>
      ),
    },
    {
      key: 'days_requested',
      header: 'Days',
      align: 'right',
      render: (row) => <span className="tabular-nums">{row.days_requested}</span>,
    },
    { key: 'status', header: 'Status', render: (row) => <StatusBadge status={row.status} /> },
    {
      key: 'actions',
      header: '',
      align: 'right',
      render: (row) =>
        // Only a pending request can still be decided.
        canDecide && row.status === 'pending' ? (
          <div className="flex justify-end gap-1">
            <Button
              size="sm"
              variant="outline"
              disabled={decide.isPending}
              onClick={() => decide.mutate({ id: row.id, status: 'approved' })}
            >
              <Check className="mr-1 h-3.5 w-3.5" />
              Approve
            </Button>
            <Button
              size="sm"
              variant="ghost"
              disabled={decide.isPending}
              onClick={() => decide.mutate({ id: row.id, status: 'rejected' })}
            >
              <X className="mr-1 h-3.5 w-3.5" />
              Reject
            </Button>
          </div>
        ) : null,
    },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Leave requests"
        description="Annual leave draws down each employee's yearly entitlement"
        actions={
          <Button onClick={() => setCreating(true)}>
            <Plus className="mr-1 h-4 w-4" />
            Request leave
          </Button>
        }
      />

      <FilterBar
        selects={[
          {
            label: 'Status',
            value: filters.status,
            onChange: (value) => setFilter('status', value),
            options: LEAVE_STATUSES,
          },
          {
            label: 'Type',
            value: filters.leave_type,
            onChange: (value) => setFilter('leave_type', value),
            options: LEAVE_TYPES,
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
        emptyTitle="No leave requests"
        emptyMessage="Requests appear here for approval."
        emptyAction={<Button onClick={() => setCreating(true)}>Request leave</Button>}
      />

      {creating && <LeaveRequestDialog onClose={() => setCreating(false)} />}
    </div>
  );
}

function LeaveRequestDialog({ onClose }: { onClose: () => void }) {
  const create = leaveRequests.useCreate({ successMessage: 'Leave requested', onSuccess: onClose });
  const { options: employeeOptions } = useEmployeeOptions();

  const form = useForm<LeaveRequestForm>({
    resolver: zodResolver(leaveRequestSchema),
    defaultValues: {
      employee_id: '',
      leave_type: 'annual',
      start_date: toDateInput(),
      end_date: toDateInput(),
      reason: '',
    },
  });

  const { errors } = form.formState;
  const onSubmit = form.handleSubmit((values) => create.mutate(toPayload(values)));

  return (
    <Dialog
      open
      onClose={onClose}
      title="Request leave"
      description="Days default to the working days in the range."
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={create.isPending}>
            {create.isPending ? 'Submitting…' : 'Submit request'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <Field label="Employee" required error={errors.employee_id?.message}>
          <Select
            options={employeeOptions}
            placeholder="Who is taking leave"
            {...form.register('employee_id')}
          />
        </Field>
        <FormGrid>
          <Field label="Leave type" required error={errors.leave_type?.message}>
            <Select options={LEAVE_TYPES} {...form.register('leave_type')} />
          </Field>
          <Field label="Days" error={errors.days_requested?.message} hint="Blank = working days">
            <Input type="number" min={1} {...form.register('days_requested')} />
          </Field>
          <Field label="Start date" required error={errors.start_date?.message}>
            <Input type="date" {...form.register('start_date')} />
          </Field>
          <Field label="End date" required error={errors.end_date?.message}>
            <Input type="date" {...form.register('end_date')} />
          </Field>
        </FormGrid>
        <Field label="Reason" error={errors.reason?.message}>
          <Textarea {...form.register('reason')} />
        </Field>
      </form>
    </Dialog>
  );
}

/** The transitions `ExpenseStatus::can_transition` accepts. */
const EXPENSE_NEXT: Record<string, Array<{ status: string; label: string; role?: boolean }>> = {
  draft: [{ status: 'submitted', label: 'Submit' }],
  submitted: [
    { status: 'approved', label: 'Approve', role: true },
    { status: 'rejected', label: 'Reject', role: true },
  ],
  approved: [{ status: 'reimbursed', label: 'Mark reimbursed', role: true }],
  rejected: [{ status: 'draft', label: 'Reopen' }],
};

export function ExpenseReportList() {
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams({ status: '' });
  const query = expenseReports.useList(params);
  const setStatus = useExpenseStatus();
  const [creating, setCreating] = useState(false);
  const [viewing, setViewing] = useState<ExpenseReport | null>(null);
  const canApprove = useAuthStore((state) => state.hasRole('hr', 'manager'));

  const columns: Column<ExpenseReport>[] = [
    {
      key: 'report_number',
      header: 'Report',
      render: (row) => <span className="font-medium text-slate-900">{row.report_number}</span>,
    },
    { key: 'description', header: 'Description', render: (row) => row.description ?? '—' },
    {
      key: 'total_amount',
      header: 'Total',
      sortable: true,
      align: 'right',
      render: (row) => (
        <span className="font-medium tabular-nums">
          {formatMoney(row.total_amount, row.currency)}
        </span>
      ),
    },
    { key: 'status', header: 'Status', render: (row) => <StatusBadge status={row.status} /> },
    {
      key: 'actions',
      header: '',
      align: 'right',
      render: (row) => {
        const options = (EXPENSE_NEXT[row.status] ?? []).filter(
          (option) => !option.role || canApprove
        );
        return (
          <div className="flex justify-end gap-1">
            {options.map((option) => (
              <Button
                key={option.status}
                size="sm"
                variant="outline"
                disabled={setStatus.isPending}
                onClick={(event) => {
                  event.stopPropagation();
                  setStatus.mutate({ id: row.id, status: option.status });
                }}
              >
                {option.label}
              </Button>
            ))}
          </div>
        );
      },
    },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Expense reports"
        description="Draft → submitted → approved → reimbursed"
        actions={
          <Button onClick={() => setCreating(true)}>
            <Plus className="mr-1 h-4 w-4" />
            New expense report
          </Button>
        }
      />

      <FilterBar
        selects={[
          {
            label: 'Status',
            value: filters.status,
            onChange: (value) => setFilter('status', value),
            options: EXPENSE_STATUSES,
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
        onRowClick={(row) => setViewing(row)}
        pagination={query.data?.pagination}
        onPageChange={setPage}
        sort={sort}
        onSortChange={setSort}
        emptyTitle="No expense reports"
        emptyMessage="Claim expenses by creating a report with one line per receipt."
        emptyAction={<Button onClick={() => setCreating(true)}>New expense report</Button>}
      />

      {creating && <ExpenseReportDialog onClose={() => setCreating(false)} />}
      {viewing && <ExpenseDetailDialog reportId={viewing.id} onClose={() => setViewing(null)} />}
    </div>
  );
}

function ExpenseDetailDialog({ reportId, onClose }: { reportId: string; onClose: () => void }) {
  const query = expenseReports.useOne(reportId);

  return (
    <Dialog
      open
      onClose={onClose}
      title={query.data?.report_number ?? 'Expense report'}
      className="max-w-2xl"
      footer={<Button onClick={onClose}>Close</Button>}
    >
      {query.isLoading ? (
        <p className="py-6 text-center text-sm text-slate-400">Loading…</p>
      ) : query.error ? (
        <ErrorState error={query.error} onRetry={query.refetch} />
      ) : query.data ? (
        <div className="space-y-4">
          <div className="flex items-center gap-3">
            <StatusBadge status={query.data.status} />
            <span className="text-sm text-slate-500">{query.data.description}</span>
          </div>
          <Table>
            <TableHeader>
              <TableRow className="hover:bg-transparent">
                <TableHead>Date</TableHead>
                <TableHead>Category</TableHead>
                <TableHead>Description</TableHead>
                {/* The point of the whole feature: approving a claim without
                    seeing what it is for was the gap. */}
                <TableHead>Receipt</TableHead>
                <TableHead className="text-right">Amount</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {query.data.lines.map((line) => (
                <TableRow key={line.id}>
                  <TableCell>{formatDate(line.expense_date)}</TableCell>
                  <TableCell className="capitalize">{line.category}</TableCell>
                  <TableCell>{line.description}</TableCell>
                  <TableCell>
                    {line.receipt_attachment_id ? (
                      <ReceiptLink attachmentId={line.receipt_attachment_id} />
                    ) : (
                      <span className="text-xs text-slate-400">None</span>
                    )}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {formatMoney(line.amount, query.data.currency)}
                  </TableCell>
                </TableRow>
              ))}
              <TableRow className="bg-slate-50 font-semibold hover:bg-slate-50">
                <TableCell colSpan={4}>Total</TableCell>
                <TableCell className="text-right tabular-nums">
                  {formatMoney(query.data.total_amount, query.data.currency)}
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </div>
      ) : null}
    </Dialog>
  );
}

function ExpenseReportDialog({ onClose }: { onClose: () => void }) {
  const create = expenseReports.useCreate({
    successMessage: 'Expense report created',
    onSuccess: onClose,
  });
  const { options: employeeOptions } = useEmployeeOptions();

  const form = useForm<ExpenseReportForm>({
    resolver: zodResolver(expenseReportSchema),
    defaultValues: {
      employee_id: '',
      description: '',
      lines: [
        {
          expense_date: toDateInput(),
          category: 'travel',
          description: '',
          amount: 0,
          receipt_attachment_id: null,
        },
      ],
    },
  });

  const { fields, append, remove } = useFieldArray({ control: form.control, name: 'lines' });
  const { errors } = form.formState;
  const lines = form.watch('lines') ?? [];
  const total = lines.reduce((sum, line) => sum + (Number(line.amount) || 0), 0);
  const linesError = (errors.lines as { message?: string } | undefined)?.message;

  const onSubmit = form.handleSubmit((values) =>
    create.mutate({
      ...values,
      lines: values.lines.map((line) => ({
        ...line,
        amount: Number(line.amount).toFixed(2),
        receipt_attachment_id: line.receipt_attachment_id || undefined,
      })),
    })
  );

  return (
    <Dialog
      open
      onClose={onClose}
      title="New expense report"
      className="max-w-3xl"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={create.isPending}>
            {create.isPending ? 'Saving…' : 'Create report'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <FormGrid>
          <Field label="Employee" required error={errors.employee_id?.message}>
            <Select
              options={employeeOptions}
              placeholder="Who is claiming"
              {...form.register('employee_id')}
            />
          </Field>
          <CurrencyField {...form.register('currency')} />
        </FormGrid>
        <Field label="Description" error={errors.description?.message}>
          <Input placeholder="What this claim covers" {...form.register('description')} />
        </Field>

        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-slate-900">Expense lines</h3>
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() =>
                append({
                  expense_date: toDateInput(),
                  category: 'travel',
                  description: '',
                  amount: 0,
                  receipt_attachment_id: null,
                })
              }
            >
              <Plus className="mr-1 h-4 w-4" />
              Add line
            </Button>
          </div>

          {linesError && (
            <p className="text-xs font-medium text-red-600" role="alert">
              {linesError}
            </p>
          )}

          {fields.map((field, index) => {
            const rowError = (errors.lines ?? [])[index] as
              | Record<string, { message?: string }>
              | undefined;

            return (
              <div key={field.id} className="flex items-start gap-2">
                <div className="w-36">
                  <Input type="date" {...form.register(`lines.${index}.expense_date`)} />
                </div>
                <div className="w-32">
                  <Input placeholder="Category" {...form.register(`lines.${index}.category`)} />
                  {rowError?.category?.message && (
                    <p className="mt-1 text-xs text-red-600">{rowError.category.message}</p>
                  )}
                </div>
                <div className="flex-1">
                  <Input placeholder="Description" {...form.register(`lines.${index}.description`)} />
                  {rowError?.description?.message && (
                    <p className="mt-1 text-xs text-red-600">{rowError.description.message}</p>
                  )}
                </div>
                <div className="w-28">
                  <Input
                    type="number"
                    step="0.01"
                    min={0.01}
                    className="text-right"
                    {...form.register(`lines.${index}.amount`)}
                  />
                  {rowError?.amount?.message && (
                    <p className="mt-1 text-xs text-red-600">{rowError.amount.message}</p>
                  )}
                </div>
                <div className="w-36">
                  <Controller
                    control={form.control}
                    name={`lines.${index}.receipt_attachment_id`}
                    render={({ field }) => (
                      <FileUpload
                        value={field.value}
                        onChange={field.onChange}
                        label="Receipt"
                      />
                    )}
                  />
                </div>
                <button
                  type="button"
                  onClick={() => remove(index)}
                  className="mt-2 rounded p-1.5 text-slate-400 hover:bg-red-50 hover:text-red-600"
                  aria-label={`Remove line ${index + 1}`}
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
            );
          })}

          <div className="flex justify-end border-t pt-2 text-sm font-semibold">
            Total: {formatMoney(total)}
          </div>
        </div>
      </form>
    </Dialog>
  );
}
