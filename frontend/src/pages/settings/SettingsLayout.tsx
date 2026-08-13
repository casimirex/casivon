import { NavLink, Outlet } from 'react-router-dom';
import { ArrowLeftRight, BookOpen, Building2, User, Users } from 'lucide-react';
import { cn } from '@/lib/utils';
import { useAuthStore } from '@/store/authStore';

const TABS = [
  { to: '/settings/profile', label: 'Your account', icon: User },
  // Both are admin-only screens; a manager can read the user list through the
  // API but has nothing to change on it, so it is not offered here.
  { to: '/settings/users', label: 'Users', icon: Users, adminOnly: true },
  { to: '/settings/company', label: 'Company', icon: Building2, adminOnly: true },
  {
    to: '/settings/exchange-rates',
    label: 'Exchange rates',
    icon: ArrowLeftRight,
    adminOnly: true,
  },
  { to: '/settings/posting', label: 'Automatic posting', icon: BookOpen, adminOnly: true },
];

export function SettingsLayout() {
  const isAdmin = useAuthStore((state) => state.hasRole('admin'));
  const tabs = TABS.filter((tab) => !tab.adminOnly || isAdmin);

  return (
    <div className="space-y-6">
      <nav className="flex gap-1 overflow-x-auto border-b border-slate-200" aria-label="Settings">
        {tabs.map((tab) => (
          <NavLink
            key={tab.to}
            to={tab.to}
            className={({ isActive }) =>
              cn(
                'flex items-center gap-2 whitespace-nowrap border-b-2 px-4 py-2.5 text-sm font-medium transition-colors',
                isActive
                  ? 'border-slate-900 text-slate-900'
                  : 'border-transparent text-slate-500 hover:text-slate-900'
              )
            }
          >
            <tab.icon className="h-4 w-4" aria-hidden />
            {tab.label}
          </NavLink>
        ))}
      </nav>

      <Outlet />
    </div>
  );
}
