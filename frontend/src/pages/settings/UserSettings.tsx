import { useState } from 'react';
import { ShieldCheck, UserCheck, UserX } from 'lucide-react';
import { PageHeader } from '@/components/common/PageHeader';
import { DataTable, type Column } from '@/components/common/DataTable';
import { FilterBar } from '@/components/common/Filters';
import { Button } from '@/components/ui/Button';
import { Badge } from '@/components/ui/Badge';
import { Select } from '@/components/ui/Select';
import { ConfirmDialog } from '@/components/ui/Dialog';
import { useListParams } from '@/hooks/useListParams';
import { users, useSetUserRole, useSetUserStatus } from '@/hooks/useSettings';
import { useAuthStore } from '@/store/authStore';
import { formatDate, humanize } from '@/lib/utils';
import { USER_ROLES, type User } from '@/types';

/** Roles that unlock a gated module, so the badge draws the eye to them. */
const PRIVILEGED = new Set(['admin', 'manager', 'accountant', 'hr']);

export function UserSettings() {
  const { params, setPage, setSort, setFilter, filters, reset } = useListParams(
    { search: '', role: '', is_active: '' },
    { defaultSort: 'created_at' }
  );
  const currentUserId = useAuthStore((state) => state.user?.id);
  const list = users.useList(params);
  const setRole = useSetUserRole();
  const setStatus = useSetUserStatus();
  const [retiring, setRetiring] = useState<User | null>(null);

  const columns: Column<User>[] = [
    {
      key: 'last_name',
      header: 'Name',
      sortable: true,
      render: (user) => (
        <div className="min-w-0">
          <p className="truncate font-medium text-slate-900">
            {user.first_name} {user.last_name}
            {user.id === currentUserId && <span className="ml-2 text-xs text-slate-400">you</span>}
          </p>
          <p className="truncate text-sm text-slate-500">{user.email}</p>
        </div>
      ),
    },
    {
      key: 'role',
      header: 'Role',
      render: (user) => (
        <div className="flex items-center gap-2">
          {PRIVILEGED.has(user.role) && <ShieldCheck className="h-4 w-4 text-slate-400" aria-hidden />}
          <Select
            aria-label={`Role for ${user.first_name} ${user.last_name}`}
            value={user.role}
            options={USER_ROLES.map((role) => ({ value: role, label: humanize(role) }))}
            // An admin demoting themselves would lock the instance out of every
            // admin-only screen, so the API refuses it and so does this.
            disabled={user.id === currentUserId || setRole.isPending}
            onChange={(event) => setRole.mutate({ id: user.id, role: event.target.value })}
            className="w-40"
          />
        </div>
      ),
    },
    {
      key: 'is_active',
      header: 'Status',
      render: (user) =>
        user.is_active === false ? (
          <Badge tone="neutral">Retired</Badge>
        ) : (
          <Badge tone="success">Active</Badge>
        ),
    },
    {
      key: 'created_at',
      header: 'Joined',
      sortable: true,
      render: (user) => formatDate(user.created_at),
    },
    {
      key: 'actions',
      header: '',
      align: 'right',
      render: (user) =>
        user.id === currentUserId ? null : user.is_active === false ? (
          <Button
            variant="outline"
            size="sm"
            onClick={() => setStatus.mutate({ id: user.id, is_active: true })}
            disabled={setStatus.isPending}
          >
            <UserCheck className="mr-1.5 h-4 w-4" />
            Restore
          </Button>
        ) : (
          <Button variant="outline" size="sm" onClick={() => setRetiring(user)}>
            <UserX className="mr-1.5 h-4 w-4" />
            Retire
          </Button>
        ),
    },
  ];

  return (
    <div className="space-y-6">
      <PageHeader
        title="Users"
        description="People register themselves; an administrator decides what they can reach."
      />

      <FilterBar
        search={{
          value: filters.search,
          onChange: (value) => setFilter('search', value),
          placeholder: 'Search by name or email',
        }}
        selects={[
          {
            label: 'Role',
            value: filters.role,
            options: USER_ROLES.map((role) => ({ value: role, label: humanize(role) })),
            onChange: (value) => setFilter('role', value),
          },
          {
            label: 'Status',
            value: filters.is_active,
            options: [
              { value: 'true', label: 'Active' },
              { value: 'false', label: 'Retired' },
            ],
            onChange: (value) => setFilter('is_active', value),
          },
        ]}
        onReset={reset}
      />

      <DataTable
        columns={columns}
        rows={list.data?.data}
        rowKey={(user) => user.id}
        isLoading={list.isLoading}
        error={list.error}
        onRetry={list.refetch}
        sort={params.sort}
        onSortChange={setSort}
        pagination={list.data?.pagination}
        onPageChange={setPage}
        emptyTitle="No one else yet"
        emptyMessage="Accounts appear here once colleagues register."
      />

      <ConfirmDialog
        open={Boolean(retiring)}
        onClose={() => setRetiring(null)}
        onConfirm={() => {
          if (retiring) setStatus.mutate({ id: retiring.id, is_active: false });
          setRetiring(null);
        }}
        title={`Retire ${retiring?.first_name} ${retiring?.last_name}?`}
        // Retiring rather than deleting: the account is referenced by every
        // document it created, and those have to keep their author.
        message="They will not be able to sign in. Everything they created stays where it is, and you can restore the account at any time."
        confirmLabel="Retire account"
      />
    </div>
  );
}
