import { create } from 'zustand';
import { revokeSession, tokenStore } from '@/api/client';
import type { SessionUser } from '@/types';

const USER_KEY = 'erp_user';

/**
 * Reads the cached user so a page refresh does not bounce an authenticated user
 * back to the login screen while `/users/me` is in flight.
 */
function restoreUser(): SessionUser | null {
  try {
    const raw = localStorage.getItem(USER_KEY);
    return raw ? (JSON.parse(raw) as SessionUser) : null;
  } catch {
    return null;
  }
}

interface AuthState {
  user: SessionUser | null;
  isAuthenticated: boolean;
  setAuth: (user: SessionUser, accessToken: string, refreshToken?: string) => void;
  setUser: (user: SessionUser) => void;
  logout: () => void;
  /** Admins bypass every check, matching `CurrentUser::require_any_role`. */
  hasRole: (...roles: string[]) => boolean;
}

export const useAuthStore = create<AuthState>((set, get) => ({
  user: restoreUser(),
  // The token is the real credential; the cached user is only for display.
  isAuthenticated: Boolean(tokenStore.access()),

  setAuth: (user, accessToken, refreshToken) => {
    tokenStore.set(accessToken, refreshToken);
    localStorage.setItem(USER_KEY, JSON.stringify(user));
    set({ user, isAuthenticated: true });
  },

  setUser: (user) => {
    localStorage.setItem(USER_KEY, JSON.stringify(user));
    set({ user });
  },

  logout: () => {
    // Fire first: `revokeSession` reads the refresh token before its first
    // await, so it has the token in hand by the time `clear()` runs below.
    // Not awaited — the user is signed out here and now, whatever the network
    // is doing.
    void revokeSession();
    tokenStore.clear();
    localStorage.removeItem(USER_KEY);
    set({ user: null, isAuthenticated: false });
  },

  hasRole: (...roles) => {
    const role = get().user?.role;
    if (!role) return false;
    return role === 'admin' || roles.includes(role);
  },
}));
