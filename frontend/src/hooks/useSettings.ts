import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { http, tokenStore, type ApiError } from '@/api/client';
import { useToast } from '@/components/ui/toast-context';
import { useAuthStore } from '@/store/authStore';
import { createResource, useAction } from './useResource';
import type {
  AuthResponse,
  AvailableCurrencies,
  FxRate,
  OrganizationSettings,
  User,
} from '@/types';

/**
 * Read-only through the generic factory: accounts are not created here (people
 * register themselves) and are retired rather than deleted, so the settings
 * screen only ever lists and updates.
 */
export const users = createResource<User>('/users', 'users');

export function useSetUserRole() {
  return useAction<User, { id: string; role: string }>(
    'users',
    ({ id, role }) => http.put<User>(`/users/${id}/role`, { role }),
    { successMessage: 'Role updated' }
  );
}

export function useSetUserStatus() {
  return useAction<User, { id: string; is_active: boolean }>(
    'users',
    ({ id, is_active }) => http.put<User>(`/users/${id}/status`, { is_active }),
    { successMessage: 'Account updated' }
  );
}

export function useOrganization() {
  return useQuery({
    queryKey: ['organization'],
    queryFn: () => http.get<OrganizationSettings>('/settings/organization'),
  });
}

export function useUpdateOrganization() {
  return useAction<OrganizationSettings, Record<string, unknown>>(
    'organization',
    (body) => http.put<OrganizationSettings>('/settings/organization', body),
    { successMessage: 'Company details saved' }
  );
}

/**
 * What a currency picker may offer: the base currency plus everything with a
 * rate on file.
 *
 * Long-lived in the cache because it changes only when an admin adds a currency,
 * and every document form mounts it.
 */
export function useCurrencies() {
  return useQuery({
    queryKey: ['currencies'],
    queryFn: () => http.get<AvailableCurrencies>('/settings/currencies'),
    staleTime: 5 * 60 * 1000,
  });
}

export function useFxRates(currency?: string) {
  return useQuery({
    queryKey: ['fx-rates', currency ?? 'all'],
    queryFn: () =>
      http.get<FxRate[]>(currency ? `/settings/fx-rates?currency=${currency}` : '/settings/fx-rates'),
  });
}

export function useUpsertFxRate() {
  const queryClient = useQueryClient();
  const toast = useToast();

  return useMutation<FxRate, ApiError, { currency: string; effective_from: string; rate: string }>({
    mutationFn: (body) => http.put<FxRate>('/settings/fx-rates', body),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['fx-rates'] });
      // Adding the first rate for a currency is what makes it selectable on
      // every document form, so the picker's list has to be refreshed too.
      queryClient.invalidateQueries({ queryKey: ['currencies'] });
      toast.success('Exchange rate saved');
    },
    onError: (error) => toast.error(error.message),
  });
}

export function useDeleteFxRate() {
  const queryClient = useQueryClient();
  const toast = useToast();

  return useMutation<unknown, ApiError, string>({
    mutationFn: (id) => http.delete(`/settings/fx-rates/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['fx-rates'] });
      queryClient.invalidateQueries({ queryKey: ['currencies'] });
      toast.success('Exchange rate removed');
    },
    onError: (error) => toast.error(error.message),
  });
}

export function useUpdateProfile() {
  const queryClient = useQueryClient();
  const toast = useToast();
  const setUser = useAuthStore((state) => state.setUser);

  return useMutation<User, ApiError, { first_name: string; last_name: string }>({
    mutationFn: (body) => http.put<User>('/users/me', body),
    onSuccess: (user) => {
      // The header and sidebar read the name from the store, so it has to be
      // refreshed here or the change looks like it did not take.
      setUser(user);
      queryClient.invalidateQueries({ queryKey: ['users'] });
      toast.success('Profile updated');
    },
    onError: (error) => toast.error(error.message),
  });
}

export function useChangePassword() {
  const toast = useToast();

  return useMutation<AuthResponse, ApiError, { current_password: string; new_password: string }>({
    mutationFn: (body) => http.put<AuthResponse>('/users/me/password', body),
    onSuccess: (data) => {
      // Changing a password ends every session, including this one. The server
      // hands back a fresh pair so the person who made the change stays signed
      // in; storing it here is what keeps them from being bounced to /login.
      tokenStore.set(data.access_token, data.refresh_token);
      toast.success('Password changed. Other sessions have been signed out.');
    },
    onError: (error) => toast.error(error.message),
  });
}
