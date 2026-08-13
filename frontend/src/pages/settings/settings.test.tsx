import { describe, expect, it, vi, beforeEach } from 'vitest';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { renderWithProviders } from '@/test/renderWithProviders';
import { UserSettings } from './UserSettings';
import { ProfileSettings } from './ProfileSettings';
import { organizationSchema } from '@/schemas';
import { http } from '@/api/client';
import { useAuthStore } from '@/store/authStore';

const admin = {
  id: 'admin-1',
  email: 'ada@erp.test',
  first_name: 'Ada',
  last_name: 'Admin',
  role: 'admin',
  email_verified: true,
};

const bob = {
  id: 'user-2',
  email: 'bob@erp.test',
  first_name: 'Bob',
  last_name: 'Clerk',
  role: 'user',
  email_verified: true,
  is_active: true,
  created_at: '2026-01-15T10:00:00Z',
};

function mockUserList() {
  return vi.spyOn(http, 'list').mockResolvedValue({
    success: true,
    data: [{ ...admin, is_active: true, created_at: '2026-01-01T10:00:00Z' }, bob],
    pagination: { page: 1, per_page: 20, total: 2, total_pages: 1 },
  });
}

describe('<UserSettings />', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useAuthStore.setState({ user: admin, isAuthenticated: true });
  });

  it('lists accounts with their role and status', async () => {
    mockUserList();
    renderWithProviders(<UserSettings />);

    expect(await screen.findByText('bob@erp.test')).toBeInTheDocument();
    expect(screen.getAllByText('Active')).not.toHaveLength(0);
  });

  it('will not let an admin change their own role', async () => {
    mockUserList();
    renderWithProviders(<UserSettings />);

    // Demoting yourself locks the instance out of every admin-only screen, so
    // the API refuses it — the control must not invite the attempt.
    const own = await screen.findByLabelText('Role for Ada Admin');
    expect(own).toBeDisabled();
    expect(screen.getByLabelText('Role for Bob Clerk')).toBeEnabled();
  });

  it('grants a role through the API', async () => {
    mockUserList();
    const put = vi.spyOn(http, 'put').mockResolvedValue({ ...bob, role: 'accountant' });
    const user = userEvent.setup();
    renderWithProviders(<UserSettings />);

    await user.selectOptions(await screen.findByLabelText('Role for Bob Clerk'), 'accountant');

    await waitFor(() => {
      expect(put).toHaveBeenCalledWith('/users/user-2/role', { role: 'accountant' });
    });
  });

  it('confirms before retiring an account, and explains what survives', async () => {
    mockUserList();
    const put = vi.spyOn(http, 'put').mockResolvedValue({ ...bob, is_active: false });
    const user = userEvent.setup();
    renderWithProviders(<UserSettings />);

    await user.click(await screen.findByRole('button', { name: /retire/i }));

    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByText(/Everything they created stays where it is/i)).toBeInTheDocument();

    await user.click(within(dialog).getByRole('button', { name: /retire account/i }));
    await waitFor(() => {
      expect(put).toHaveBeenCalledWith('/users/user-2/status', { is_active: false });
    });
  });

  it('offers no retire button for your own account', async () => {
    mockUserList();
    renderWithProviders(<UserSettings />);

    await screen.findByText('bob@erp.test');
    // Bob's row has one; Ada's does not.
    expect(screen.getAllByRole('button', { name: /retire/i })).toHaveLength(1);
  });
});

describe('<ProfileSettings />', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useAuthStore.setState({ user: admin, isAuthenticated: true });
  });

  it('shows the email as read-only, since changing it needs verification', () => {
    renderWithProviders(<ProfileSettings />);

    expect(screen.getByLabelText(/email/i)).toBeDisabled();
    expect(screen.getByLabelText(/first name/i)).toBeEnabled();
  });

  it('sends only the password fields the API expects', async () => {
    const put = vi.spyOn(http, 'put').mockResolvedValue({
      access_token: 'new-access',
      refresh_token: 'new-refresh',
      token_type: 'Bearer',
      expires_in: 900,
      user: admin,
    });
    const user = userEvent.setup();
    renderWithProviders(<ProfileSettings />);

    await user.type(screen.getByLabelText(/current password/i), 'supersecret1');
    await user.type(screen.getByLabelText(/^new password/i), 'a-brand-new-one');
    await user.type(screen.getByLabelText(/confirm new password/i), 'a-brand-new-one');
    await user.click(screen.getByRole('button', { name: /change password/i }));

    await waitFor(() => {
      expect(put).toHaveBeenCalledWith('/users/me/password', {
        current_password: 'supersecret1',
        new_password: 'a-brand-new-one',
      });
    });
    // The server issues a fresh pair because the change ends every session;
    // storing it is what keeps this tab signed in.
    expect(localStorage.getItem('access_token')).toBe('new-access');
  });

  it('refuses a new password that matches the current one', async () => {
    const put = vi.spyOn(http, 'put');
    const user = userEvent.setup();
    renderWithProviders(<ProfileSettings />);

    await user.type(screen.getByLabelText(/current password/i), 'supersecret1');
    await user.type(screen.getByLabelText(/^new password/i), 'supersecret1');
    await user.type(screen.getByLabelText(/confirm new password/i), 'supersecret1');
    await user.click(screen.getByRole('button', { name: /change password/i }));

    expect(await screen.findByText(/must be different/i)).toBeInTheDocument();
    expect(put).not.toHaveBeenCalled();
  });
});

describe('organizationSchema', () => {
  const complete = {
    name: 'Globex',
    legal_name: '',
    email: '',
    phone: '',
    website: '',
    tax_number: '',
    address_line1: '',
    address_line2: '',
    city: '',
    postal_code: '',
    country: '',
    default_currency: 'usd',
    default_dispatch_warehouse_id: '',
  };

  it('keeps blank optional fields rather than dropping them', () => {
    const parsed = organizationSchema.parse(complete);

    // The API reads `''` as "clear this field"; dropping the key would mean
    // "leave it alone", so clearing a field would silently do nothing.
    expect(parsed).toHaveProperty('phone', '');
    expect(parsed).toHaveProperty('website', '');
  });

  it('upper-cases the currency code', () => {
    expect(organizationSchema.parse(complete).default_currency).toBe('USD');
  });

  it('requires a company name', () => {
    const result = organizationSchema.safeParse({ ...complete, name: '' });
    expect(result.success).toBe(false);
  });

  it('validates email and website only when they carry a value', () => {
    expect(organizationSchema.safeParse({ ...complete, email: 'nope' }).success).toBe(false);
    expect(organizationSchema.safeParse({ ...complete, website: 'nope' }).success).toBe(false);
    expect(organizationSchema.safeParse({ ...complete, email: '', website: '' }).success).toBe(true);
  });

  it('takes a warehouse id or an empty string, and nothing else', () => {
    const warehouse = '11111111-1111-4111-8111-111111111111';

    // Empty is how dispatch is switched off, so it has to survive parsing for
    // the same reason the other clearable fields do.
    expect(organizationSchema.parse(complete)).toHaveProperty(
      'default_dispatch_warehouse_id',
      ''
    );
    expect(
      organizationSchema.safeParse({ ...complete, default_dispatch_warehouse_id: warehouse })
        .success
    ).toBe(true);
    expect(
      organizationSchema.safeParse({ ...complete, default_dispatch_warehouse_id: 'MAIN' }).success
    ).toBe(false);
  });
});
