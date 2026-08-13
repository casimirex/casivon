import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '@/test/renderWithProviders';
import { Login } from './Login';
import { http } from '@/api/client';
import { useAuthStore } from '@/store/authStore';

/**
 * Covers the wiring the type checker cannot: that Zod errors reach the screen,
 * that a successful login stores the session, and that a rejected login shows
 * the server's message instead of a blank form.
 */
describe('<Login />', () => {
  beforeEach(() => {
    useAuthStore.getState().logout();
  });

  it('shows the sign-in form by default', () => {
    renderWithProviders(<Login />);
    expect(screen.getByRole('heading', { name: /sign in/i })).toBeInTheDocument();
    expect(screen.queryByLabelText(/first name/i)).not.toBeInTheDocument();
  });

  it('reports invalid input inline and never calls the API', async () => {
    const post = vi.spyOn(http, 'post');
    const user = userEvent.setup();
    renderWithProviders(<Login />);

    await user.type(screen.getByLabelText(/email/i), 'not-an-email');
    await user.click(screen.getByRole('button', { name: /sign in/i }));

    expect(await screen.findByText(/enter a valid email address/i)).toBeInTheDocument();
    expect(screen.getByText(/password is required/i)).toBeInTheDocument();
    expect(post).not.toHaveBeenCalled();
  });

  it('stores the session when the credentials are accepted', async () => {
    vi.spyOn(http, 'post').mockResolvedValue({
      access_token: 'access-token',
      refresh_token: 'refresh-token',
      token_type: 'Bearer',
      expires_in: 900,
      user: {
        id: 'a1',
        email: 'ada@erp.test',
        first_name: 'Ada',
        last_name: 'Admin',
        role: 'admin',
      },
    });

    const user = userEvent.setup();
    renderWithProviders(<Login />);

    await user.type(screen.getByLabelText(/email/i), 'ada@erp.test');
    await user.type(screen.getByLabelText(/password/i), 'supersecret1');
    await user.click(screen.getByRole('button', { name: /sign in/i }));

    await waitFor(() => {
      expect(useAuthStore.getState().isAuthenticated).toBe(true);
    });
    expect(useAuthStore.getState().user?.role).toBe('admin');
    expect(localStorage.getItem('access_token')).toBe('access-token');
  });

  it("surfaces the server's rejection next to the form", async () => {
    vi.spyOn(http, 'post').mockRejectedValue(
      Object.assign(new Error('Invalid credentials'), { status: 401 })
    );

    const user = userEvent.setup();
    renderWithProviders(<Login />);

    await user.type(screen.getByLabelText(/email/i), 'ada@erp.test');
    await user.type(screen.getByLabelText(/password/i), 'wrong-password');
    await user.click(screen.getByRole('button', { name: /sign in/i }));

    expect(await screen.findByRole('alert')).toHaveTextContent('Invalid credentials');
    expect(useAuthStore.getState().isAuthenticated).toBe(false);
  });

  it('switches to registration and asks for the extra fields', async () => {
    const user = userEvent.setup();
    renderWithProviders(<Login />);

    await user.click(screen.getByRole('button', { name: /sign up/i }));

    expect(screen.getByLabelText(/first name/i)).toBeInTheDocument();
    expect(screen.getByText(/first account created becomes the administrator/i)).toBeInTheDocument();
  });

  it('enforces the 8-character password minimum when registering', async () => {
    const post = vi.spyOn(http, 'post');
    const user = userEvent.setup();
    renderWithProviders(<Login />);

    await user.click(screen.getByRole('button', { name: /sign up/i }));
    await user.type(screen.getByLabelText(/first name/i), 'Ada');
    await user.type(screen.getByLabelText(/last name/i), 'Admin');
    await user.type(screen.getByLabelText(/email/i), 'ada@erp.test');
    await user.type(screen.getByLabelText(/password/i), 'short');
    await user.click(screen.getByRole('button', { name: /create account/i }));

    expect(await screen.findByText(/at least 8 characters/i)).toBeInTheDocument();
    expect(post).not.toHaveBeenCalled();
  });
});
