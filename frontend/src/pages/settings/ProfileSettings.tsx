import { useEffect } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { PageHeader } from '@/components/common/PageHeader';
import { Field, FormGrid } from '@/components/common/Field';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Badge } from '@/components/ui/Badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card';
import { useAuthStore } from '@/store/authStore';
import { useChangePassword, useUpdateProfile } from '@/hooks/useSettings';
import { humanize } from '@/lib/utils';
import {
  changePasswordSchema,
  profileSchema,
  type ChangePasswordForm,
  type ProfileForm,
} from '@/schemas';

export function ProfileSettings() {
  const user = useAuthStore((state) => state.user);
  const updateProfile = useUpdateProfile();
  const changePassword = useChangePassword();

  const details = useForm<ProfileForm>({
    resolver: zodResolver(profileSchema),
    defaultValues: { first_name: user?.first_name ?? '', last_name: user?.last_name ?? '' },
  });

  // The store rehydrates from `/users/me` after a reload, so the form has to
  // pick the name up when it arrives rather than staying blank.
  useEffect(() => {
    if (user) details.reset({ first_name: user.first_name, last_name: user.last_name });
    // Resetting on every render of `details` would fight the user's typing.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [user?.first_name, user?.last_name]);

  const password = useForm<ChangePasswordForm>({
    resolver: zodResolver(changePasswordSchema),
    defaultValues: { current_password: '', new_password: '', confirm_password: '' },
  });

  return (
    <div className="space-y-6">
      <PageHeader
        title="Your account"
        description="Your name as colleagues see it, and the password you sign in with."
        badge={user && <Badge tone="neutral">{humanize(user.role)}</Badge>}
      />

      <Card>
        <CardHeader>
          <CardTitle>Details</CardTitle>
        </CardHeader>
        <CardContent>
          <form
            onSubmit={details.handleSubmit((values) => updateProfile.mutate(values))}
            className="space-y-4"
            noValidate
          >
            <FormGrid>
              <Field
                label="First name"
                required
                htmlFor="first_name"
                error={details.formState.errors.first_name?.message}
              >
                <Input id="first_name" {...details.register('first_name')} />
              </Field>
              <Field
                label="Last name"
                required
                htmlFor="last_name"
                error={details.formState.errors.last_name?.message}
              >
                <Input id="last_name" {...details.register('last_name')} />
              </Field>
            </FormGrid>

            {/* Changing an address would need re-verification, which does not
                exist yet, so it is shown but not editable. */}
            <Field label="Email" htmlFor="email" hint="Contact an administrator to change this">
              <Input id="email" value={user?.email ?? ''} disabled readOnly />
            </Field>

            <div className="flex justify-end">
              <Button type="submit" disabled={updateProfile.isPending}>
                {updateProfile.isPending ? 'Saving…' : 'Save changes'}
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Password</CardTitle>
          <p className="text-sm text-slate-500">
            Changing it signs out every other device. You will stay signed in here.
          </p>
        </CardHeader>
        <CardContent>
          <form
            onSubmit={password.handleSubmit((values) =>
              changePassword.mutate(
                { current_password: values.current_password, new_password: values.new_password },
                { onSuccess: () => password.reset() }
              )
            )}
            className="space-y-4"
            noValidate
          >
            <Field
              label="Current password"
              required
              htmlFor="current_password"
              error={password.formState.errors.current_password?.message}
            >
              <Input
                id="current_password"
                type="password"
                autoComplete="current-password"
                {...password.register('current_password')}
              />
            </Field>

            <FormGrid>
              <Field
                label="New password"
                required
                htmlFor="new_password"
                hint="At least 8 characters"
                error={password.formState.errors.new_password?.message}
              >
                <Input
                  id="new_password"
                  type="password"
                  autoComplete="new-password"
                  {...password.register('new_password')}
                />
              </Field>
              <Field
                label="Confirm new password"
                required
                htmlFor="confirm_password"
                error={password.formState.errors.confirm_password?.message}
              >
                <Input
                  id="confirm_password"
                  type="password"
                  autoComplete="new-password"
                  {...password.register('confirm_password')}
                />
              </Field>
            </FormGrid>

            <div className="flex justify-end">
              <Button type="submit" disabled={changePassword.isPending}>
                {changePassword.isPending ? 'Changing…' : 'Change password'}
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
