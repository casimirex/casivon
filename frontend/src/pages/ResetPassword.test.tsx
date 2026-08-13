import { describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '@/test/renderWithProviders';
import { ResetPassword } from './ResetPassword';
import { ForgotPassword } from './ForgotPassword';
import { http, ApiError } from '@/api/client';

describe('<ResetPassword />', () => {
  it('explains itself when the link arrived without a token', () => {
    renderWithProviders(<ResetPassword />, { route: '/reset-password' });

    expect(screen.getByText(/link is incomplete/i)).toBeInTheDocument();
    expect(screen.queryByLabelText(/new password/i)).not.toBeInTheDocument();
  });

  it('refuses to submit when the two passwords differ', async () => {
    const post = vi.spyOn(http, 'post');
    const user = userEvent.setup();
    renderWithProviders(<ResetPassword />, { route: '/reset-password?token=abc123' });

    await user.type(screen.getByLabelText(/^new password/i), 'a-good-password');
    await user.type(screen.getByLabelText(/confirm new password/i), 'a-different-one');
    await user.click(screen.getByRole('button', { name: /change password/i }));

    expect(await screen.findByText(/passwords do not match/i)).toBeInTheDocument();
    expect(post).not.toHaveBeenCalled();
  });

  it('enforces the 8-character minimum the API also enforces', async () => {
    const post = vi.spyOn(http, 'post');
    const user = userEvent.setup();
    renderWithProviders(<ResetPassword />, { route: '/reset-password?token=abc123' });

    await user.type(screen.getByLabelText(/^new password/i), 'short');
    await user.type(screen.getByLabelText(/confirm new password/i), 'short');
    await user.click(screen.getByRole('button', { name: /change password/i }));

    expect(await screen.findByText(/at least 8 characters/i)).toBeInTheDocument();
    expect(post).not.toHaveBeenCalled();
  });

  it('sends the token with the password, and never the confirmation', async () => {
    const post = vi.spyOn(http, 'post').mockResolvedValue({ password_changed: true });
    const user = userEvent.setup();
    renderWithProviders(<ResetPassword />, { route: '/reset-password?token=abc123' });

    await user.type(screen.getByLabelText(/^new password/i), 'a-good-password');
    await user.type(screen.getByLabelText(/confirm new password/i), 'a-good-password');
    await user.click(screen.getByRole('button', { name: /change password/i }));

    await waitFor(() => {
      expect(post).toHaveBeenCalledWith('/auth/reset-password', {
        token: 'abc123',
        password: 'a-good-password',
      });
    });
  });

  it('offers a new link when the server says this one is spent', async () => {
    vi.spyOn(http, 'post').mockRejectedValue(
      new ApiError('This reset link is invalid or has expired', 401)
    );
    const user = userEvent.setup();
    renderWithProviders(<ResetPassword />, { route: '/reset-password?token=abc123' });

    await user.type(screen.getByLabelText(/^new password/i), 'a-good-password');
    await user.type(screen.getByLabelText(/confirm new password/i), 'a-good-password');
    await user.click(screen.getByRole('button', { name: /change password/i }));

    expect(await screen.findByRole('alert')).toHaveTextContent('invalid or has expired');
    expect(screen.getByRole('link', { name: /request a new link/i })).toBeInTheDocument();
  });
});

describe('<ForgotPassword />', () => {
  it("shows the server's non-committal acknowledgement rather than inventing one", async () => {
    // The API says the same thing whether or not the address is registered;
    // the screen must not narrow that down.
    vi.spyOn(http, 'post').mockResolvedValue({
      message: 'If that address has an account, a reset link is on its way.',
    });
    const user = userEvent.setup();
    renderWithProviders(<ForgotPassword />);

    await user.type(screen.getByLabelText(/email/i), 'ada@erp.test');
    await user.click(screen.getByRole('button', { name: /send reset link/i }));

    expect(await screen.findByText(/if that address has an account/i)).toBeInTheDocument();
    expect(screen.queryByText(/we sent|check your inbox at/i)).not.toBeInTheDocument();
  });

  it('validates the address before calling the API', async () => {
    const post = vi.spyOn(http, 'post');
    const user = userEvent.setup();
    renderWithProviders(<ForgotPassword />);

    await user.type(screen.getByLabelText(/email/i), 'not-an-email');
    await user.click(screen.getByRole('button', { name: /send reset link/i }));

    expect(await screen.findByText(/enter a valid email address/i)).toBeInTheDocument();
    expect(post).not.toHaveBeenCalled();
  });
});
