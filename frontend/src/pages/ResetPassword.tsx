import { Link, useNavigate, useSearchParams } from 'react-router-dom';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { useMutation } from '@tanstack/react-query';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Field } from '@/components/common/Field';
import { AuthShell } from '@/components/common/AuthShell';
import { http, type ApiError } from '@/api/client';
import { useToast } from '@/components/ui/toast-context';
import { resetPasswordSchema, type ResetPasswordForm } from '@/schemas';

interface ResetPasswordResponse {
  password_changed: boolean;
}

export function ResetPassword() {
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const toast = useToast();
  const token = params.get('token') ?? '';

  const form = useForm<ResetPasswordForm>({
    resolver: zodResolver(resetPasswordSchema),
    defaultValues: { password: '', confirm_password: '' },
  });

  const submit = useMutation<ResetPasswordResponse, ApiError, ResetPasswordForm>({
    // `confirm_password` never leaves the browser — the API takes one password.
    mutationFn: (values) =>
      http.post<ResetPasswordResponse>('/auth/reset-password', {
        token,
        password: values.password,
      }),
    onSuccess: () => {
      // The reset ends every existing session, so there is nothing to log into
      // automatically — send them through the front door with the new password.
      toast.success('Password changed. Sign in with your new password.');
      navigate('/login', { replace: true });
    },
  });

  // A link that arrived without its token cannot be recovered from here.
  if (!token) {
    return (
      <AuthShell
        title="This link is incomplete"
        footer={
          <Link to="/forgot-password" className="font-medium text-primary hover:underline">
            Request a new link
          </Link>
        }
      >
        <p className="text-sm text-slate-600">
          The reset link is missing its token. It may have been cut short by your email client —
          copying the whole address usually fixes it.
        </p>
      </AuthShell>
    );
  }

  return (
    <AuthShell
      title="Choose a new password"
      subtitle="Signing in elsewhere will need the new password."
      footer={
        <Link to="/login" className="font-medium text-primary hover:underline">
          Back to sign in
        </Link>
      }
    >
      <form
        onSubmit={form.handleSubmit((values) => submit.mutate(values))}
        className="space-y-4"
        noValidate
      >
        <Field
          label="New password"
          required
          htmlFor="password"
          hint="At least 8 characters"
          error={form.formState.errors.password?.message}
        >
          <Input
            id="password"
            type="password"
            autoComplete="new-password"
            {...form.register('password')}
          />
        </Field>

        <Field
          label="Confirm new password"
          required
          htmlFor="confirm_password"
          error={form.formState.errors.confirm_password?.message}
        >
          <Input
            id="confirm_password"
            type="password"
            autoComplete="new-password"
            {...form.register('confirm_password')}
          />
        </Field>

        {submit.isError && (
          <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2" role="alert">
            <p className="text-sm text-red-700">{submit.error.message}</p>
            {submit.error.status === 401 && (
              <Link
                to="/forgot-password"
                className="mt-1 inline-block text-sm font-medium text-red-800 hover:underline"
              >
                Request a new link
              </Link>
            )}
          </div>
        )}

        <Button type="submit" className="w-full" disabled={submit.isPending}>
          {submit.isPending ? 'Saving…' : 'Change password'}
        </Button>
      </form>
    </AuthShell>
  );
}
