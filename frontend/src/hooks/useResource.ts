import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationOptions,
  type UseQueryOptions,
} from '@tanstack/react-query';
import { http, ApiError } from '@/api/client';
import { useToast } from '@/components/ui/toast-context';
import type { ListParams, PaginatedResponse, Uuid } from '@/types';

/**
 * Request body for documents whose create/update payload is not simply a subset
 * of the entity — anything carrying a `lines` array, or money re-serialised to
 * strings. Zod has already validated the shape by the time it gets here.
 */
export type DocumentBody = Record<string, unknown>;

/**
 * Every module exposes the same five operations over a REST collection, so they
 * are defined once here. A module calls `createResource<Entity, Detail>(path)`
 * and gets typed hooks that already handle cache invalidation and toasts.
 */
export interface Resource<TEntity, TDetail = TEntity, TCreate = unknown, TUpdate = unknown> {
  path: string;
  key: string;
  useList: (
    params?: ListParams,
    options?: Partial<UseQueryOptions<PaginatedResponse<TEntity>, ApiError>>
  ) => ReturnType<typeof useQuery<PaginatedResponse<TEntity>, ApiError>>;
  useOne: (
    id: Uuid | undefined,
    options?: Partial<UseQueryOptions<TDetail, ApiError>>
  ) => ReturnType<typeof useQuery<TDetail, ApiError>>;
  useCreate: (
    options?: MutationOptions<TDetail, TCreate>
  ) => ReturnType<typeof useMutation<TDetail, ApiError, TCreate>>;
  useUpdate: (
    options?: MutationOptions<TDetail, { id: Uuid; body: TUpdate }>
  ) => ReturnType<typeof useMutation<TDetail, ApiError, { id: Uuid; body: TUpdate }>>;
  useRemove: (
    options?: MutationOptions<unknown, Uuid>
  ) => ReturnType<typeof useMutation<unknown, ApiError, Uuid>>;
}

type MutationOptions<TData, TVars> = Omit<
  UseMutationOptions<TData, ApiError, TVars>,
  'mutationFn'
> & {
  /** Shown as a success toast. Omit for silent mutations. */
  successMessage?: string;
};

export function createResource<
  TEntity,
  TDetail = TEntity,
  TCreate = Partial<TEntity>,
  TUpdate = Partial<TEntity>,
>(path: string, key: string): Resource<TEntity, TDetail, TCreate, TUpdate> {
  function useInvalidate() {
    const queryClient = useQueryClient();
    return () => queryClient.invalidateQueries({ queryKey: [key] });
  }

  return {
    path,
    key,

    useList(params, options) {
      return useQuery<PaginatedResponse<TEntity>, ApiError>({
        queryKey: [key, 'list', params ?? {}],
        queryFn: () => http.list<TEntity>(path, params),
        ...options,
      });
    },

    useOne(id, options) {
      return useQuery<TDetail, ApiError>({
        queryKey: [key, 'detail', id],
        queryFn: () => http.get<TDetail>(`${path}/${id}`),
        // Detail pages mount before the id is parsed out of the route.
        enabled: Boolean(id),
        ...options,
      });
    },

    useCreate(options) {
      const invalidate = useInvalidate();
      const toast = useToast();
      const { successMessage, onSuccess, onError, ...rest } = options ?? {};

      return useMutation<TDetail, ApiError, TCreate>({
        mutationFn: (body) => http.post<TDetail>(path, body),
        // Callback args are forwarded verbatim so this keeps working across
        // TanStack Query releases that add parameters to the signature.
        onSuccess: (...args) => {
          invalidate();
          if (successMessage) toast.success(successMessage);
          onSuccess?.(...args);
        },
        onError: (...args) => {
          toast.error(args[0].message);
          onError?.(...args);
        },
        ...rest,
      });
    },

    useUpdate(options) {
      const invalidate = useInvalidate();
      const toast = useToast();
      const { successMessage, onSuccess, onError, ...rest } = options ?? {};

      return useMutation<TDetail, ApiError, { id: Uuid; body: TUpdate }>({
        mutationFn: ({ id, body }) => http.put<TDetail>(`${path}/${id}`, body),
        onSuccess: (...args) => {
          invalidate();
          if (successMessage) toast.success(successMessage);
          onSuccess?.(...args);
        },
        onError: (...args) => {
          toast.error(args[0].message);
          onError?.(...args);
        },
        ...rest,
      });
    },

    useRemove(options) {
      const invalidate = useInvalidate();
      const toast = useToast();
      const { successMessage, onSuccess, onError, ...rest } = options ?? {};

      return useMutation<unknown, ApiError, Uuid>({
        mutationFn: (id) => http.delete(`${path}/${id}`),
        onSuccess: (...args) => {
          invalidate();
          toast.success(successMessage ?? 'Deleted');
          onSuccess?.(...args);
        },
        onError: (...args) => {
          toast.error(args[0].message);
          onError?.(...args);
        },
        ...rest,
      });
    },
  };
}

/**
 * For the endpoints that are not plain CRUD — status transitions, conversions,
 * approvals. Invalidates the module's cache and reports through the toast system.
 */
export function useAction<TData, TVars>(
  key: string,
  mutationFn: (vars: TVars) => Promise<TData>,
  options?: MutationOptions<TData, TVars> & { invalidateKeys?: string[] }
) {
  const queryClient = useQueryClient();
  const toast = useToast();
  const { successMessage, invalidateKeys, onSuccess, onError, ...rest } = options ?? {};

  return useMutation<TData, ApiError, TVars>({
    mutationFn,
    onSuccess: (...args) => {
      for (const k of [key, ...(invalidateKeys ?? [])]) {
        queryClient.invalidateQueries({ queryKey: [k] });
      }
      if (successMessage) toast.success(successMessage);
      onSuccess?.(...args);
    },
    onError: (...args) => {
      toast.error(args[0].message);
      onError?.(...args);
    },
    ...rest,
  });
}
