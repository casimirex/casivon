import { useEffect, useState } from 'react';
import { Outlet, useLocation } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { Sidebar } from './Sidebar';
import { Header } from './Header';
import { CurrencyProvider } from '@/components/common/CurrencyProvider';
import { VerifyEmailPrompt } from '@/components/common/VerifyEmailPrompt';
import { http } from '@/api/client';
import { useAuthStore } from '@/store/authStore';
import type { User } from '@/types';

export function AppLayout() {
  const [navOpen, setNavOpen] = useState(false);
  const location = useLocation();
  const setUser = useAuthStore((state) => state.setUser);

  // The cached user gets us rendering immediately; this refreshes it (and the
  // role the sidebar filters on) from the server.
  const { data } = useQuery({
    queryKey: ['me'],
    queryFn: () => http.get<User>('/users/me'),
    staleTime: 5 * 60_000,
  });

  useEffect(() => {
    if (data) setUser(data);
  }, [data, setUser]);

  // Close the mobile drawer whenever navigation happens.
  useEffect(() => setNavOpen(false), [location.pathname]);

  return (
    <div className="min-h-screen bg-slate-50">
      {/* Sets the currency `formatMoney` labels amounts with. Renders nothing. */}
      <CurrencyProvider />
      <Sidebar open={navOpen} onClose={() => setNavOpen(false)} />
      <div className="lg:ml-64">
        <Header onOpenNav={() => setNavOpen(true)} />
        {/* Below the header rather than over the content: it is a prompt, not
            an error, and nothing is blocked on it. */}
        <VerifyEmailPrompt />
        <main className="p-4 sm:p-6 lg:p-8">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
