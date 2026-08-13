import { useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { Clock, Plus } from 'lucide-react';
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
import { CurrencyField } from '@/components/common/CurrencyField';
import { useListParams } from '@/hooks/useListParams';
import {
  projects,
  tasks,
  useCreateTimeEntry,
  useProjectStatus,
  useProjectTasks,
  useProjectTimeEntries,
  useTaskStatus,
} from '@/hooks/useProjects';
import { useCompanyOptions } from '@/hooks/useCrm';
import { useEmployeeOptions } from '@/hooks/useHr';
import { formatDate, formatMoney, humanize, toDateInput } from '@/lib/utils';
import { toPayload } from '@/schemas/common';
import {
  projectSchema,
  taskSchema,
  timeEntrySchema,
  type ProjectForm,
  type TaskForm,
  type TimeEntryForm,
} from '@/schemas';
import {
  PRIORITIES,
  PROJECT_STATUSES,
  type Project,
  type Task,
  type TimeEntry,
} from '@/types';

export function ProjectList() {
  const navigate = useNavigate();
  const { params, setPage, sort, setSort, filters, setFilter, reset } = useListParams({
    search: '',
    status: '',
    priority: '',
  });
  const query = projects.useList(params);
  const [creating, setCreating] = useState(false);

  const columns: Column<Project>[] = [
    {
      key: 'name',
      header: 'Project',
      sortable: true,
      render: (row) => (
        <div>
          <p className="font-medium text-slate-900">{row.name}</p>
          <p className="font-mono text-xs text-slate-500">{row.project_code}</p>
        </div>
      ),
    },
    { key: 'status', header: 'Status', render: (row) => <StatusBadge status={row.status} /> },
    { key: 'priority', header: 'Priority', render: (row) => <StatusBadge status={row.priority} /> },
    {
      key: 'progress_percent',
      header: 'Progress',
      sortable: true,
      render: (row) => <ProgressBar value={row.progress_percent} />,
    },
    {
      key: 'budget',
      header: 'Budget',
      align: 'right',
      render: (row) => <span className="tabular-nums">{formatMoney(row.budget, row.currency)}</span>,
    },
    { key: 'end_date', header: 'Ends', sortable: true, render: (row) => formatDate(row.end_date) },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Projects"
        description="Progress rolls up automatically from each project's tasks"
        actions={
          <Button onClick={() => setCreating(true)}>
            <Plus className="mr-1 h-4 w-4" />
            New project
          </Button>
        }
      />

      <FilterBar
        search={{
          value: filters.search,
          onChange: (value) => setFilter('search', value),
          placeholder: 'Search projects…',
        }}
        selects={[
          {
            label: 'Status',
            value: filters.status,
            onChange: (value) => setFilter('status', value),
            options: PROJECT_STATUSES,
          },
          {
            label: 'Priority',
            value: filters.priority,
            onChange: (value) => setFilter('priority', value),
            options: PRIORITIES,
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
        onRowClick={(row) => navigate(`/projects/${row.id}`)}
        pagination={query.data?.pagination}
        onPageChange={setPage}
        sort={sort}
        onSortChange={setSort}
        emptyTitle="No projects"
        emptyMessage="Create a project, then break it into tasks."
        emptyAction={<Button onClick={() => setCreating(true)}>New project</Button>}
      />

      {creating && <ProjectDialog onClose={() => setCreating(false)} />}
    </div>
  );
}

function ProgressBar({ value }: { value: number }) {
  return (
    <div className="flex items-center gap-2">
      <div className="h-1.5 w-24 overflow-hidden rounded-full bg-slate-200">
        <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${value}%` }} />
      </div>
      <span className="text-xs tabular-nums text-slate-500">{value}%</span>
    </div>
  );
}

const NEXT_STATUS: Record<string, Array<{ status: string; label: string }>> = {
  planning: [
    { status: 'active', label: 'Start project' },
    { status: 'cancelled', label: 'Cancel' },
  ],
  active: [
    { status: 'on_hold', label: 'Put on hold' },
    { status: 'completed', label: 'Complete' },
    { status: 'cancelled', label: 'Cancel' },
  ],
  on_hold: [
    { status: 'active', label: 'Resume' },
    { status: 'cancelled', label: 'Cancel' },
  ],
};

export function ProjectDetail() {
  const { id } = useParams<{ id: string }>();
  const query = projects.useOne(id);
  const setStatus = useProjectStatus();
  const [addingTask, setAddingTask] = useState(false);
  const [loggingTime, setLoggingTime] = useState(false);

  if (query.isLoading) return <DetailSkeleton />;
  if (query.error) return <ErrorState error={query.error} onRetry={query.refetch} />;
  if (!query.data) return null;

  const project = query.data;
  const transitions = NEXT_STATUS[project.status] ?? [];
  // The server refuses new work on completed or cancelled projects.
  const acceptsWork = ['planning', 'active'].includes(project.status);

  return (
    <div className="space-y-6">
      <PageHeader
        title={project.name}
        description={project.project_code}
        backTo="/projects"
        backLabel="Back to projects"
        badge={<StatusBadge status={project.status} />}
        actions={
          <>
            {transitions.map((transition) => (
              <Button
                key={transition.status}
                variant="outline"
                disabled={setStatus.isPending}
                onClick={() => setStatus.mutate({ id: project.id, status: transition.status })}
              >
                {transition.label}
              </Button>
            ))}
            {acceptsWork && (
              <>
                <Button variant="outline" onClick={() => setLoggingTime(true)}>
                  <Clock className="mr-1 h-4 w-4" />
                  Log time
                </Button>
                <Button onClick={() => setAddingTask(true)}>
                  <Plus className="mr-1 h-4 w-4" />
                  New task
                </Button>
              </>
            )}
          </>
        }
      />

      <SummaryGrid
        items={[
          { label: 'Progress', value: `${project.progress_percent}%` },
          {
            label: 'Tasks',
            value: `${project.task_summary.done} / ${project.task_summary.total} done`,
          },
          { label: 'Budget', value: formatMoney(project.budget, project.currency) },
          { label: 'Billable hours', value: Number(project.billable_hours).toFixed(2) },
        ]}
      />

      <TaskBoard projectId={project.id} />

      <ProjectTimeEntries projectId={project.id} />

      {addingTask && <TaskDialog projectId={project.id} onClose={() => setAddingTask(false)} />}
      {loggingTime && <TimeEntryDialog projectId={project.id} onClose={() => setLoggingTime(false)} />}
    </div>
  );
}

/** Kanban columns keyed by the statuses the task state machine allows. */
const BOARD_COLUMNS = ['todo', 'in_progress', 'review', 'done'] as const;

/** Which status each column can move a task to next, per `TaskStatus`. */
const TASK_MOVES: Record<string, Array<{ status: string; label: string }>> = {
  todo: [{ status: 'in_progress', label: 'Start' }],
  in_progress: [{ status: 'review', label: 'To review' }],
  review: [
    { status: 'done', label: 'Done' },
    { status: 'in_progress', label: 'Back' },
  ],
  done: [{ status: 'in_progress', label: 'Reopen' }],
};

function TaskBoard({ projectId }: { projectId: string }) {
  const query = useProjectTasks(projectId, { per_page: 200 });
  const move = useTaskStatus();

  if (query.isLoading) {
    return (
      <Card>
        <CardContent className="py-12 text-center text-sm text-slate-400">Loading tasks…</CardContent>
      </Card>
    );
  }
  if (query.error) return <ErrorState error={query.error} onRetry={query.refetch} />;

  const allTasks = query.data?.data ?? [];
  if (allTasks.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="text-base">Task board</CardTitle>
        </CardHeader>
        <CardContent>
          <EmptyState title="No tasks yet" message="Break the project down into tasks to track progress." />
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Task board</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
          {BOARD_COLUMNS.map((status) => {
            const columnTasks = allTasks.filter((task) => task.status === status);

            return (
              <div key={status} className="rounded-lg bg-slate-50 p-3">
                <div className="mb-3 flex items-center justify-between">
                  <h4 className="text-sm font-semibold text-slate-700">{humanize(status)}</h4>
                  <Badge tone="muted">{columnTasks.length}</Badge>
                </div>
                <div className="space-y-2">
                  {columnTasks.map((task) => (
                    <TaskCard key={task.id} task={task} onMove={move.mutate} busy={move.isPending} />
                  ))}
                  {columnTasks.length === 0 && (
                    <p className="py-4 text-center text-xs text-slate-400">Nothing here</p>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
}

function TaskCard({
  task,
  onMove,
  busy,
}: {
  task: Task;
  onMove: (vars: { id: string; status: string }) => void;
  busy: boolean;
}) {
  const moves = TASK_MOVES[task.status] ?? [];

  return (
    <div className="rounded-md border bg-white p-3 shadow-sm">
      <div className="flex items-start justify-between gap-2">
        <p className="text-sm font-medium text-slate-900">{task.title}</p>
        <StatusBadge status={task.priority} />
      </div>
      <p className="mt-1 font-mono text-xs text-slate-400">{task.task_code}</p>

      {task.due_date && (
        <p className="mt-2 text-xs text-slate-500">Due {formatDate(task.due_date)}</p>
      )}

      {Number(task.actual_hours ?? 0) > 0 && (
        <p className="mt-1 text-xs text-slate-500">
          {Number(task.actual_hours).toFixed(2)}h logged
          {task.estimated_hours ? ` of ${Number(task.estimated_hours).toFixed(2)}h` : ''}
        </p>
      )}

      {moves.length > 0 && (
        <div className="mt-3 flex gap-1">
          {moves.map((option) => (
            <Button
              key={option.status}
              size="sm"
              variant="outline"
              className="h-7 px-2 text-xs"
              disabled={busy}
              onClick={() => onMove({ id: task.id, status: option.status })}
            >
              {option.label}
            </Button>
          ))}
        </div>
      )}
    </div>
  );
}

function ProjectTimeEntries({ projectId }: { projectId: string }) {
  const query = useProjectTimeEntries(projectId, { per_page: 10 });

  const columns: Column<TimeEntry>[] = [
    { key: 'entry_date', header: 'Date', render: (row) => formatDate(row.entry_date) },
    {
      key: 'hours',
      header: 'Hours',
      align: 'right',
      render: (row) => <span className="tabular-nums">{Number(row.hours).toFixed(2)}</span>,
    },
    {
      key: 'is_billable',
      header: 'Billable',
      render: (row) =>
        row.is_billable ? <Badge tone="success">Billable</Badge> : <Badge tone="muted">Internal</Badge>,
    },
    { key: 'description', header: 'Notes', render: (row) => row.description ?? '—' },
  ];

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">Recent time entries</CardTitle>
      </CardHeader>
      <CardContent>
        <DataTable
          columns={columns}
          rows={query.data?.data}
          rowKey={(row) => row.id}
          isLoading={query.isLoading}
          error={query.error}
          onRetry={query.refetch}
          emptyTitle="No time logged"
          emptyMessage="Logged hours roll into each task's actual hours."
        />
      </CardContent>
    </Card>
  );
}

function ProjectDialog({ onClose }: { onClose: () => void }) {
  const create = projects.useCreate({ successMessage: 'Project created', onSuccess: onClose });
  const { options: customerOptions } = useCompanyOptions();

  const form = useForm<ProjectForm>({
    resolver: zodResolver(projectSchema),
    defaultValues: {
      name: '',
      description: '',
      priority: 'medium',
      start_date: toDateInput(),
      end_date: '',
    },
  });

  const { errors } = form.formState;
  const onSubmit = form.handleSubmit((values) => create.mutate(toPayload(values, ['budget'])));

  return (
    <Dialog
      open
      onClose={onClose}
      title="New project"
      className="max-w-2xl"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={create.isPending}>
            {create.isPending ? 'Saving…' : 'Create project'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <Field label="Name" required error={errors.name?.message}>
          <Input {...form.register('name')} />
        </Field>
        <FormGrid>
          <Field label="Customer" error={errors.customer_id?.message}>
            <Select options={customerOptions} placeholder="Internal project" {...form.register('customer_id')} />
          </Field>
          <Field label="Priority" required error={errors.priority?.message}>
            <Select options={PRIORITIES} {...form.register('priority')} />
          </Field>
          <Field label="Start date" error={errors.start_date?.message}>
            <Input type="date" {...form.register('start_date')} />
          </Field>
          <Field label="End date" error={errors.end_date?.message}>
            <Input type="date" {...form.register('end_date')} />
          </Field>
          <Field label="Budget" error={errors.budget?.message}>
            <Input type="number" step="0.01" min={0} {...form.register('budget')} />
          </Field>
          <CurrencyField {...form.register('currency')} />
        </FormGrid>
        <Field label="Description" error={errors.description?.message}>
          <Textarea {...form.register('description')} />
        </Field>
      </form>
    </Dialog>
  );
}

function TaskDialog({ projectId, onClose }: { projectId: string; onClose: () => void }) {
  const create = tasks.useCreate({ successMessage: 'Task created', onSuccess: onClose });

  const form = useForm<TaskForm>({
    resolver: zodResolver(taskSchema),
    defaultValues: {
      project_id: projectId,
      title: '',
      description: '',
      priority: 'medium',
      start_date: '',
      due_date: '',
    },
  });

  const { errors } = form.formState;
  const onSubmit = form.handleSubmit((values) =>
    create.mutate(toPayload(values, ['estimated_hours']))
  );

  return (
    <Dialog
      open
      onClose={onClose}
      title="New task"
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={create.isPending}>
            {create.isPending ? 'Saving…' : 'Create task'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <Field label="Title" required error={errors.title?.message}>
          <Input {...form.register('title')} />
        </Field>
        <FormGrid>
          <Field label="Priority" required error={errors.priority?.message}>
            <Select options={PRIORITIES} {...form.register('priority')} />
          </Field>
          <Field label="Estimated hours" error={errors.estimated_hours?.message}>
            <Input type="number" step="0.25" min={0} {...form.register('estimated_hours')} />
          </Field>
          <Field label="Start date" error={errors.start_date?.message}>
            <Input type="date" {...form.register('start_date')} />
          </Field>
          <Field label="Due date" error={errors.due_date?.message}>
            <Input type="date" {...form.register('due_date')} />
          </Field>
        </FormGrid>
        <Field label="Description" error={errors.description?.message}>
          <Textarea {...form.register('description')} />
        </Field>
      </form>
    </Dialog>
  );
}

function TimeEntryDialog({ projectId, onClose }: { projectId: string; onClose: () => void }) {
  const create = useCreateTimeEntry();
  const { options: employeeOptions } = useEmployeeOptions();
  const projectTasks = useProjectTasks(projectId, { per_page: 200 });

  const taskOptions = (projectTasks.data?.data ?? []).map((task) => ({
    value: task.id,
    label: `${task.task_code} — ${task.title}`,
  }));

  const form = useForm<TimeEntryForm>({
    resolver: zodResolver(timeEntrySchema),
    defaultValues: {
      task_id: '',
      employee_id: '',
      entry_date: toDateInput(),
      hours: 1,
      description: '',
      is_billable: true,
    },
  });

  const { errors } = form.formState;
  const onSubmit = form.handleSubmit((values) =>
    create.mutate(
      { ...values, hours: Number(values.hours).toFixed(2) },
      { onSuccess: onClose }
    )
  );

  return (
    <Dialog
      open
      onClose={onClose}
      title="Log time"
      description="Hours roll into the task's actual hours and the project totals."
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button onClick={onSubmit} disabled={create.isPending}>
            {create.isPending ? 'Logging…' : 'Log time'}
          </Button>
        </>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4" noValidate>
        <Field label="Task" required error={errors.task_id?.message}>
          <Select options={taskOptions} placeholder="Which task" {...form.register('task_id')} />
        </Field>
        <Field label="Employee" required error={errors.employee_id?.message}>
          <Select options={employeeOptions} placeholder="Who did the work" {...form.register('employee_id')} />
        </Field>
        <FormGrid>
          <Field label="Date" required error={errors.entry_date?.message}>
            <Input type="date" {...form.register('entry_date')} />
          </Field>
          <Field label="Hours" required error={errors.hours?.message} hint="Max 24 per entry">
            <Input type="number" step="0.25" min={0.25} max={24} {...form.register('hours')} />
          </Field>
        </FormGrid>
        <label className="flex items-center gap-2 text-sm text-slate-700">
          <input type="checkbox" className="h-4 w-4 rounded border-input" {...form.register('is_billable')} />
          Billable to the customer
        </label>
        <Field label="Notes" error={errors.description?.message}>
          <Textarea {...form.register('description')} />
        </Field>
      </form>
    </Dialog>
  );
}

