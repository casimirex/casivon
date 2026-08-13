import { Link } from 'react-router-dom';
import {
  AlertTriangle,
  DollarSign,
  FolderKanban,
  Package,
  Receipt,
  TrendingUp,
  Users,
} from 'lucide-react';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card';
import { CardSkeleton } from '@/components/ui/Skeleton';
import { StatusBadge } from '@/components/ui/Badge';
import { PageHeader } from '@/components/common/PageHeader';
import { EmptyState } from '@/components/common/States';
import { contacts, usePipeline } from '@/hooks/useCrm';
import { invoices } from '@/hooks/useSales';
import { useLowStock, useStockValuation } from '@/hooks/useInventory';
import { projects } from '@/hooks/useProjects';
import { leaveRequests } from '@/hooks/useHr';
import { formatDate, formatMoney } from '@/lib/utils';

export function Dashboard() {
  // Each widget owns its query, so a slow one never blocks the rest.
  const outstanding = invoices.useList({ status: 'sent', per_page: 5, sort: 'due_date' });
  const overdue = invoices.useList({ status: 'overdue', per_page: 5, sort: 'due_date' });
  const contactCount = contacts.useList({ per_page: 1 });
  const activeProjects = projects.useList({ status: 'active', per_page: 5 });
  const pendingLeave = leaveRequests.useList({ status: 'pending', per_page: 5 });
  const lowStock = useLowStock({ per_page: 5 });
  const valuation = useStockValuation();
  const pipeline = usePipeline();

  const loading = outstanding.isLoading || contactCount.isLoading || valuation.isLoading;

  const pipelineValue = (pipeline.data ?? []).reduce(
    (sum, stage) => sum + Number(stage.value ?? 0),
    0
  );

  const stats = [
    {
      label: 'Open pipeline',
      value: formatMoney(pipelineValue),
      icon: TrendingUp,
      to: '/crm',
    },
    {
      label: 'Awaiting payment',
      value: String(outstanding.data?.pagination.total ?? 0),
      icon: Receipt,
      to: '/sales/invoices',
    },
    {
      label: 'Stock at cost',
      value: formatMoney(valuation.data?.total_value),
      icon: Package,
      to: '/inventory/warehouses',
    },
    {
      label: 'Contacts',
      value: String(contactCount.data?.pagination.total ?? 0),
      icon: Users,
      to: '/crm',
    },
  ];

  return (
    <div className="space-y-6">
      <PageHeader title="Dashboard" description="Live figures across every module" />

      {loading ? (
        <CardSkeleton />
      ) : (
        <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
          {stats.map((stat) => (
            <Link key={stat.label} to={stat.to} className="block">
              <Card className="transition-shadow hover:shadow-md">
                <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                  <CardTitle className="text-sm font-medium text-slate-600">{stat.label}</CardTitle>
                  <stat.icon className="h-4 w-4 text-slate-400" />
                </CardHeader>
                <CardContent>
                  <p className="text-2xl font-bold text-slate-900">{stat.value}</p>
                </CardContent>
              </Card>
            </Link>
          ))}
        </div>
      )}

      {(overdue.data?.pagination.total ?? 0) > 0 && (
        <div className="flex items-start gap-3 rounded-md border border-red-200 bg-red-50 p-4">
          <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-red-500" />
          <div>
            <p className="text-sm font-medium text-red-900">
              {overdue.data?.pagination.total} invoice
              {(overdue.data?.pagination.total ?? 0) > 1 ? 's are' : ' is'} overdue
            </p>
            <Link to="/sales/invoices?status=overdue" className="text-sm text-red-700 underline">
              Review overdue invoices
            </Link>
          </div>
        </div>
      )}

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Invoices awaiting payment</CardTitle>
          </CardHeader>
          <CardContent>
            {!outstanding.data?.data.length ? (
              <EmptyState title="Nothing outstanding" message="Every sent invoice has been settled." />
            ) : (
              <ul className="divide-y">
                {outstanding.data.data.map((invoice) => (
                  <li key={invoice.id} className="flex items-center justify-between py-3">
                    <div>
                      <Link
                        to={`/sales/invoices/${invoice.id}`}
                        className="text-sm font-medium text-slate-900 hover:underline"
                      >
                        {invoice.invoice_number}
                      </Link>
                      <p className="text-xs text-slate-500">Due {formatDate(invoice.due_date)}</p>
                    </div>
                    <span className="text-sm font-medium tabular-nums">
                      {formatMoney(invoice.amount_due, invoice.currency)}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">Pipeline by stage</CardTitle>
          </CardHeader>
          <CardContent>
            {!pipeline.data?.length ? (
              <EmptyState title="No open opportunities" message="Deals in progress appear here." />
            ) : (
              <ul className="space-y-3">
                {pipeline.data.map((stage) => (
                  <li key={stage.stage} className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <StatusBadge status={stage.stage} />
                      <span className="text-xs text-slate-500">{stage.count} deal(s)</span>
                    </div>
                    <span className="text-sm font-medium tabular-nums">
                      {formatMoney(stage.value)}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">Active projects</CardTitle>
          </CardHeader>
          <CardContent>
            {!activeProjects.data?.data.length ? (
              <EmptyState title="No active projects" message="Start a project to see it here." />
            ) : (
              <ul className="divide-y">
                {activeProjects.data.data.map((project) => (
                  <li key={project.id} className="flex items-center justify-between gap-4 py-3">
                    <div className="min-w-0">
                      <Link
                        to={`/projects/${project.id}`}
                        className="block truncate text-sm font-medium text-slate-900 hover:underline"
                      >
                        {project.name}
                      </Link>
                      <p className="font-mono text-xs text-slate-500">{project.project_code}</p>
                    </div>
                    <div className="flex shrink-0 items-center gap-2">
                      <div className="h-1.5 w-16 overflow-hidden rounded-full bg-slate-200">
                        <div
                          className="h-full rounded-full bg-primary"
                          style={{ width: `${project.progress_percent}%` }}
                        />
                      </div>
                      <span className="text-xs tabular-nums text-slate-500">
                        {project.progress_percent}%
                      </span>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-base">Needs attention</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <AttentionRow
              icon={Package}
              label="Products below reorder level"
              count={lowStock.data?.pagination.total ?? 0}
              to="/inventory/movements"
            />
            <AttentionRow
              icon={Users}
              label="Leave requests awaiting a decision"
              count={pendingLeave.data?.pagination.total ?? 0}
              to="/hr/leave-requests"
            />
            <AttentionRow
              icon={DollarSign}
              label="Overdue invoices"
              count={overdue.data?.pagination.total ?? 0}
              to="/sales/invoices"
            />
            <AttentionRow
              icon={FolderKanban}
              label="Active projects"
              count={activeProjects.data?.pagination.total ?? 0}
              to="/projects"
            />
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

function AttentionRow({
  icon: Icon,
  label,
  count,
  to,
}: {
  icon: typeof Package;
  label: string;
  count: number;
  to: string;
}) {
  return (
    <Link
      to={to}
      className="flex items-center justify-between rounded-md px-2 py-2 transition-colors hover:bg-slate-50"
    >
      <span className="flex items-center gap-2 text-sm text-slate-600">
        <Icon className="h-4 w-4 text-slate-400" />
        {label}
      </span>
      <span
        className={`text-sm font-semibold tabular-nums ${
          count > 0 ? 'text-slate-900' : 'text-slate-400'
        }`}
      >
        {count}
      </span>
    </Link>
  );
}
