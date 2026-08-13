import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '@/test/renderWithProviders';
import { VerifyEmail } from './VerifyEmail';
import { VerifyEmailPrompt } from '@/components/common/VerifyEmailPrompt';
import { http } from '@/api/client';
import { useAuthStore } from '@/store/authStore';

const unverified = {
  id: 'user-1',
  email: 'lisa@erp.test',
  first_name: 'Lisa',
  last_name: 'Simpson',
  role: 'user',
  email_verified: false,
};

describe('<VerifyEmail />', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useAuthStore.setState({ user: null, isAuthenticated: false });
  });

  /// `MemoryRouter` reads its location from here, not from `window`.
  const withToken = { route: '/verify-email?token=abc123' };

  it('spends the token on arrival, without asking again', async () => {
    const post = vi
      .spyOn(http, 'post')
      .mockResolvedValue({ email: 'lisa@erp.test', email_verified: true } as never);

    renderWithProviders(<VerifyEmail />, withToken);

    expect(await screen.findByText('Address confirmed')).toBeInTheDocument();
    expect(screen.getByText(/lisa@erp.test is confirmed/)).toBeInTheDocument();
    // The click in the inbox was the intent; a confirm button would add a step
    // with nothing behind it. And it must fire exactly once.
    expect(post).toHaveBeenCalledTimes(1);
    expect(post).toHaveBeenCalledWith('/auth/verify-email', { token: 'abc123' });
  });

  it('explains a spent or expired link instead of failing silently', async () => {
    vi.spyOn(http, 'post').mockRejectedValue(
      Object.assign(new Error('This verification link is invalid or has expired'), { status: 401 })
    );

    renderWithProviders(<VerifyEmail />, withToken);

    expect(await screen.findByText('This link is no longer valid')).toBeInTheDocument();
    expect(screen.getByText(/ask for a new one/)).toBeInTheDocument();
  });

  it('says so when the link arrived without its token', async () => {
    const post = vi.spyOn(http, 'post');

    renderWithProviders(<VerifyEmail />, { route: '/verify-email' });

    expect(await screen.findByText('This link is incomplete')).toBeInTheDocument();
    expect(post).not.toHaveBeenCalled();
  });

  it('clears the prompt for whoever is signed in here', async () => {
    useAuthStore.setState({ user: unverified, isAuthenticated: true });
    vi.spyOn(http, 'post').mockResolvedValue({
      email: 'lisa@erp.test',
      email_verified: true,
    } as never);

    renderWithProviders(<VerifyEmail />, withToken);

    await screen.findByText('Address confirmed');
    await waitFor(() => expect(useAuthStore.getState().user?.email_verified).toBe(true));
  });
});

describe('<VerifyEmailPrompt />', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useAuthStore.setState({ user: unverified, isAuthenticated: true });
  });

  it('asks an unverified user to confirm, and can send another link', async () => {
    const post = vi
      .spyOn(http, 'post')
      .mockResolvedValue({ message: 'If that address has an unverified account…' } as never);

    renderWithProviders(<VerifyEmailPrompt />);

    expect(screen.getByText('lisa@erp.test')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /Send another link/ }));

    expect(post).toHaveBeenCalledWith('/auth/resend-verification', { email: 'lisa@erp.test' });
  });

  it('stays out of the way once the address is confirmed', () => {
    useAuthStore.setState({ user: { ...unverified, email_verified: true } });
    renderWithProviders(<VerifyEmailPrompt />);

    expect(screen.queryByText('lisa@erp.test')).not.toBeInTheDocument();
  });

  it('can be dismissed, because nothing is gated on it', async () => {
    renderWithProviders(<VerifyEmailPrompt />);

    await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }));

    // Accounts that predate verification are unverified too — a banner that
    // could not be put aside would follow them forever.
    expect(screen.queryByText('lisa@erp.test')).not.toBeInTheDocument();
  });
});
