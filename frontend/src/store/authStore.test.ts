import { describe, expect, it, vi, beforeEach } from 'vitest';
import axios from 'axios';
import { useAuthStore } from './authStore';
import { tokenStore } from '@/api/client';

const user = {
  id: 'a1',
  email: 'ada@erp.test',
  first_name: 'Ada',
  last_name: 'Admin',
  role: 'admin',
  email_verified: true,
};

describe('authStore', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it('stores the session and reports the role', () => {
    useAuthStore.getState().setAuth(user, 'access-token', 'refresh-token');

    expect(useAuthStore.getState().isAuthenticated).toBe(true);
    expect(useAuthStore.getState().hasRole('accountant')).toBe(true); // admins pass everything
  });

  it('asks the server to revoke the refresh token on sign-out', async () => {
    const post = vi.spyOn(axios, 'post').mockResolvedValue({ data: {} });
    useAuthStore.getState().setAuth(user, 'access-token', 'refresh-token');

    useAuthStore.getState().logout();

    // Clearing localStorage alone would leave the refresh token usable for its
    // full seven days.
    expect(post).toHaveBeenCalledWith(
      expect.stringContaining('/auth/logout'),
      { refresh_token: 'refresh-token' },
      expect.anything()
    );
    expect(useAuthStore.getState().isAuthenticated).toBe(false);
    expect(tokenStore.access()).toBeNull();
  });

  it('signs out locally even when the server cannot be reached', async () => {
    vi.spyOn(axios, 'post').mockRejectedValue(new Error('Network down'));
    useAuthStore.getState().setAuth(user, 'access-token', 'refresh-token');

    useAuthStore.getState().logout();

    expect(useAuthStore.getState().isAuthenticated).toBe(false);
    expect(useAuthStore.getState().user).toBeNull();
  });

  it('does not call the server when there is no refresh token to revoke', () => {
    const post = vi.spyOn(axios, 'post');
    useAuthStore.getState().setAuth(user, 'access-token');

    useAuthStore.getState().logout();

    expect(post).not.toHaveBeenCalled();
  });
});
