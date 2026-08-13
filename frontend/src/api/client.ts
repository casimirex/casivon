import axios, { AxiosError, type AxiosRequestConfig } from 'axios';
import type {
  ApiErrorBody,
  ApiResponse,
  AuthResponse,
  ListParams,
  PaginatedResponse,
} from '@/types';

const ACCESS_TOKEN_KEY = 'access_token';
const REFRESH_TOKEN_KEY = 'refresh_token';

export const tokenStore = {
  access: () => localStorage.getItem(ACCESS_TOKEN_KEY),
  refresh: () => localStorage.getItem(REFRESH_TOKEN_KEY),
  set(access: string, refresh?: string) {
    localStorage.setItem(ACCESS_TOKEN_KEY, access);
    if (refresh) localStorage.setItem(REFRESH_TOKEN_KEY, refresh);
  },
  clear() {
    localStorage.removeItem(ACCESS_TOKEN_KEY);
    localStorage.removeItem(REFRESH_TOKEN_KEY);
  },
};

const api = axios.create({
  baseURL: import.meta.env.VITE_API_URL || 'http://localhost:8080/api/v1',
  headers: { 'Content-Type': 'application/json' },
});

api.interceptors.request.use((config) => {
  const token = tokenStore.access();
  if (token) {
    config.headers.Authorization = `Bearer ${token}`;
  }
  return config;
});

/**
 * The error every hook and page can rely on: a human-readable `message` lifted
 * out of the API's `{ success: false, error: { code, message } }` envelope, plus
 * the status so callers can branch on 404 / 409 / 422.
 */
export class ApiError extends Error {
  readonly status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
  }

  /** 409 and 422 are the server rejecting the *content* of a valid request. */
  get isValidation(): boolean {
    return this.status === 422 || this.status === 409;
  }

  get isNotFound(): boolean {
    return this.status === 404;
  }

  get isForbidden(): boolean {
    return this.status === 403;
  }
}

function toApiError(error: unknown): ApiError {
  if (error instanceof ApiError) return error;

  const axiosError = error as AxiosError<ApiErrorBody>;
  const status = axiosError.response?.status ?? 0;
  const message =
    axiosError.response?.data?.error?.message ??
    (status === 0
      ? 'Cannot reach the server. Is the backend running?'
      : axiosError.message) ??
    'Something went wrong';

  return new ApiError(message, status);
}

// A single in-flight refresh, so a burst of 401s produces one refresh call
// rather than one per request.
let refreshInFlight: Promise<string> | null = null;

async function refreshAccessToken(): Promise<string> {
  const refreshToken = tokenStore.refresh();
  if (!refreshToken) throw new ApiError('Session expired', 401);

  refreshInFlight ??= axios
    .post<ApiResponse<AuthResponse>>(
      `${api.defaults.baseURL}/auth/refresh`,
      { refresh_token: refreshToken },
      { headers: { 'Content-Type': 'application/json' } }
    )
    .then(({ data }) => {
      tokenStore.set(data.data.access_token, data.data.refresh_token);
      return data.data.access_token;
    })
    .finally(() => {
      refreshInFlight = null;
    });

  return refreshInFlight;
}

/**
 * Tells the server to revoke this session's refresh token.
 *
 * Clearing localStorage alone only hides the credential: the refresh token
 * stays valid for its full seven days, so anyone who captured it keeps a
 * working session. Failures are swallowed — the local sign-out must go ahead
 * even if the request cannot be delivered.
 */
export async function revokeSession(): Promise<void> {
  const refreshToken = tokenStore.refresh();
  if (!refreshToken) return;

  try {
    await axios.post(
      `${api.defaults.baseURL}/auth/logout`,
      { refresh_token: refreshToken },
      { headers: { 'Content-Type': 'application/json' } }
    );
  } catch {
    // Offline, or the token had already expired. Either way there is nothing
    // the user can do about it and nothing worth interrupting them for.
  }
}

api.interceptors.response.use(
  (response) => response,
  async (error: AxiosError<ApiErrorBody>) => {
    const original = error.config as (AxiosRequestConfig & { _retried?: boolean }) | undefined;
    const isAuthCall = original?.url?.includes('/auth/');

    // One transparent refresh-and-retry; a second 401 means the session is gone.
    if (error.response?.status === 401 && original && !original._retried && !isAuthCall) {
      original._retried = true;
      try {
        const token = await refreshAccessToken();
        original.headers = { ...original.headers, Authorization: `Bearer ${token}` };
        return await api.request(original);
      } catch {
        tokenStore.clear();
        if (window.location.pathname !== '/login') {
          window.location.href = '/login';
        }
      }
    }

    return Promise.reject(toApiError(error));
  }
);

/** Serialises list params, dropping the ones the user left blank. */
function toQuery(params?: ListParams): Record<string, string | number | boolean> | undefined {
  if (!params) return undefined;
  return Object.fromEntries(
    Object.entries(params).filter(
      ([, value]) => value !== undefined && value !== null && value !== ''
    )
  ) as Record<string, string | number | boolean>;
}

/**
 * Thin wrappers that unwrap the API envelope, so callers work with the payload
 * itself instead of `response.data.data`.
 */
export const http = {
  async get<T>(url: string, params?: ListParams): Promise<T> {
    const { data } = await api.get<ApiResponse<T>>(url, { params: toQuery(params) });
    return data.data;
  },

  /** Paginated GETs keep their envelope — the caller needs `pagination`. */
  async list<T>(url: string, params?: ListParams): Promise<PaginatedResponse<T>> {
    const { data } = await api.get<PaginatedResponse<T>>(url, { params: toQuery(params) });
    return data;
  },

  async post<T>(url: string, body?: unknown): Promise<T> {
    const { data } = await api.post<ApiResponse<T>>(url, body ?? {});
    return data.data;
  },

  /**
   * Posts a file as `multipart/form-data`.
   *
   * The instance sets `Content-Type: application/json` for every request, which
   * is right for all of them but this one — and here it must not merely be
   * changed but *removed*, so that the browser writes its own header with the
   * multipart boundary in it. A boundary cannot be guessed in advance, so
   * hand-writing this header produces a body the server cannot parse.
   */
  async upload<T>(url: string, file: File): Promise<T> {
    const form = new FormData();
    form.append('file', file);

    const { data } = await api.post<ApiResponse<T>>(url, form, {
      headers: { 'Content-Type': undefined },
    });
    return data.data;
  },

  async put<T>(url: string, body?: unknown): Promise<T> {
    const { data } = await api.put<ApiResponse<T>>(url, body ?? {});
    return data.data;
  },

  async delete<T = { deleted: boolean }>(url: string): Promise<T> {
    const { data } = await api.delete<ApiResponse<T>>(url);
    return data.data;
  },
};

export default api;
