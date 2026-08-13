import { NavLink } from 'react-router-dom';
import {
  Boxes,
  Briefcase,
  Calculator,
  FolderKanban,
  LayoutDashboard,
  LogOut,
  Package,
  Receipt,
  Settings,
  ShoppingCart,
  Users,
  X,
} from 'lucide-react';
import { cn, initials } from '@/lib/utils';
import { useAuthStore } from '@/store/authStore';

interface NavItem {
  path: string;
  label: string;
  icon: typeof LayoutDashboard;
  /** Roles allowed to see the item. Admins always see everything. */
  roles?: string[];
  children?: Array<{ path: string; label: string; roles?: string[] }>;
}

const NAV: NavItem[] = [
  { path: '/', label: 'Dashboard', icon: LayoutDashboard },
  { path: '/crm', label: 'CRM', icon: Users },
  {
    path: '/sales',
    label: 'Sales',
    icon: ShoppingCart,
    children: [
      { path: '/sales/quotes', label: 'Quotes' },
      { path: '/sales/orders', label: 'Orders' },
      { path: '/sales/invoices', label: 'Invoices' },
      { path: '/sales/payments', label: 'Payments' },
      { path: '/sales/credit-notes', label: 'Credit notes' },
    ],
  },
  {
    path: '/inventory',
    label: 'Inventory',
    icon: Package,
    children: [
      { path: '/inventory/products', label: 'Products' },
      { path: '/inventory/warehouses', label: 'Warehouses' },
      { path: '/inventory/movements', label: 'Stock movements' },
      { path: '/inventory/boms', label: 'Bills of materials' },
    ],
  },
  {
    path: '/purchasing',
    label: 'Purchasing',
    icon: Receipt,
    children: [
      { path: '/purchasing/vendors', label: 'Vendors' },
      { path: '/purchasing/purchase-orders', label: 'Purchase orders' },
      { path: '/purchasing/goods-receipts', label: 'Goods receipts' },
      { path: '/purchasing/purchase-returns', label: 'Purchase returns' },
    ],
  },
  {
    path: '/accounting',
    label: 'Accounting',
    icon: Calculator,
    roles: ['accountant', 'manager'],
    children: [
      { path: '/accounting/accounts', label: 'Chart of accounts' },
      { path: '/accounting/ledger', label: 'General ledger' },
      { path: '/accounting/bank-accounts', label: 'Bank accounts' },
      { path: '/accounting/tax-rates', label: 'Tax rates' },
      { path: '/accounting/reports', label: 'Reports' },
    ],
  },
  {
    path: '/hr',
    label: 'HR',
    icon: Briefcase,
    children: [
      { path: '/hr/employees', label: 'Employees' },
      { path: '/hr/leave-requests', label: 'Leave requests' },
      { path: '/hr/expense-reports', label: 'Expense reports' },
    ],
  },
  { path: '/projects', label: 'Projects', icon: FolderKanban },
  {
    // Everyone gets their own account page; the admin-only tabs inside are
    // filtered by `SettingsLayout`.
    path: '/settings',
    label: 'Settings',
    icon: Settings,
    children: [
      { path: '/settings/profile', label: 'Your account' },
      { path: '/settings/users', label: 'Users', roles: ['admin'] },
      { path: '/settings/company', label: 'Company', roles: ['admin'] },
    ],
  },
];

export function Sidebar({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { user, logout, hasRole } = useAuthStore();

  const visible = NAV.filter((item) => !item.roles || hasRole(...item.roles));

  return (
    <>
      {/* Backdrop only exists on mobile, where the sidebar overlays the page. */}
      {open && (
        <div
          className="fixed inset-0 z-30 bg-slate-900/50 lg:hidden"
          onClick={onClose}
          aria-hidden="true"
        />
      )}

      <aside
        className={cn(
          'fixed left-0 top-0 z-40 flex h-screen w-64 flex-col bg-slate-900 text-white transition-transform duration-200 lg:translate-x-0',
          open ? 'translate-x-0' : '-translate-x-full'
        )}
      >
        <div className="flex items-center justify-between border-b border-slate-800 px-6 py-4">
          <div className="flex items-center gap-2">
            <Boxes className="h-6 w-6" />
            <h1 className="text-lg font-bold tracking-tight">ERP System</h1>
          </div>
          <button
            onClick={onClose}
            className="rounded p-1 text-slate-400 hover:bg-slate-800 hover:text-white lg:hidden"
            aria-label="Close navigation"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        <nav className="flex-1 space-y-1 overflow-y-auto p-3">
          {visible.map((item) => (
            <div key={item.path}>
              <NavLink
                to={item.path}
                // Only the dashboard should match exactly; the rest own their subtree.
                end={item.path === '/'}
                onClick={onClose}
                className={({ isActive }) =>
                  cn(
                    'flex items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition-colors',
                    isActive
                      ? 'bg-slate-800 text-white'
                      : 'text-slate-400 hover:bg-slate-800/60 hover:text-white'
                  )
                }
              >
                <item.icon className="h-5 w-5 shrink-0" />
                {item.label}
              </NavLink>

              {item.children && (
                <div className="ml-6 mt-0.5 space-y-0.5 border-l border-slate-800 pl-3">
                  {item.children
                    .filter((child) => !child.roles || hasRole(...child.roles))
                    .map((child) => (
                    <NavLink
                      key={child.path}
                      to={child.path}
                      onClick={onClose}
                      className={({ isActive }) =>
                        cn(
                          'block rounded px-3 py-1.5 text-sm transition-colors',
                          isActive
                            ? 'text-white'
                            : 'text-slate-500 hover:text-slate-200'
                        )
                      }
                    >
                      {child.label}
                    </NavLink>
                  ))}
                </div>
              )}
            </div>
          ))}
        </nav>

        <div className="border-t border-slate-800 p-3">
          <div className="mb-2 flex items-center gap-3 rounded-lg px-3 py-2">
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-slate-700 text-xs font-semibold">
              {initials(user?.first_name, user?.last_name)}
            </div>
            <div className="min-w-0">
              <p className="truncate text-sm font-medium">
                {user?.first_name} {user?.last_name}
              </p>
              <p className="truncate text-xs capitalize text-slate-500">{user?.role}</p>
            </div>
          </div>
          <button
            onClick={logout}
            className="flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium text-slate-400 transition-colors hover:bg-slate-800 hover:text-white"
          >
            <LogOut className="h-5 w-5" />
            Sign out
          </button>
        </div>
      </aside>
    </>
  );
}

