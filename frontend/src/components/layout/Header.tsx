import { Menu } from 'lucide-react';
import { useAuthStore } from '@/store/authStore';
import { GlobalSearch } from '@/components/common/GlobalSearch';

export function Header({ onOpenNav }: { onOpenNav: () => void }) {
  const user = useAuthStore((state) => state.user);

  return (
    <header className="sticky top-0 z-20 flex h-16 items-center gap-4 border-b bg-white px-4 sm:px-8">
      <button
        onClick={onOpenNav}
        className="rounded p-2 text-slate-600 hover:bg-slate-100 lg:hidden"
        aria-label="Open navigation"
      >
        <Menu className="h-5 w-5" />
      </button>

      <GlobalSearch />

      {/* Gives way to the search box on narrow screens, where the greeting is
          the least useful thing in the bar. */}
      <h2 className="ml-auto hidden truncate text-sm text-slate-600 lg:block">
        Welcome back,{' '}
        <span className="font-semibold text-slate-900">{user?.first_name ?? 'there'}</span>
      </h2>
    </header>
  );
}
