import { useForm } from 'react-hook-form';
import { Link } from 'react-router-dom';
import { zodResolver } from '@hookform/resolvers/zod';
import { useMutation } from '@tanstack/react-query';
import { MailCheck } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Field } from '@/components/common/Field';
import { AuthShell } from '@/components/common/AuthShell';
import { http, type ApiError } from '@/api/client';
import { forgotPasswordSchema, type ForgotPasswordForm } from '@/schemas';

interface ForgotPasswordResponse {
  message: string;
}

export function ForgotPassword() {
  const form = useForm<ForgotPasswordForm>({
    resolver: zodResolver(forgotPasswordSchema),
    defaultValues: { email: '' },
  });

  const submit = useMutation<ForgotPasswordResponse, ApiError, ForgotPasswordForm>({
    mutationFn: (values) => http.post<ForgotPasswordResponse>('/auth/forgot-password', values),
  });

  // The server answers the same way for a registered and an unregistered
  // address; showing anything more specific here would undo that.
  if (submit.isSuccess) {
    return (
      <AuthShell
        title="Check your email"
        footer={
          <Link to="/login" className="font-medium text-primary hover:underline">
            Back to sign in
          </Link>
        }
      >
        <div className="flex gap-3 rounded-md border border-slate-200 bg-slate-50 p-3">
          <MailCheck className="h-5 w-5 shrink-0 text-slate-500" aria-hidden />
          <p className="text-sm text-slate-700">{submit.data.message}</p>
        </div>
        <p className="mt-3 text-sm text-slate-500">
          The link expires in an hour and can be used once.
        </p>
      </AuthShell>
    );
  }

  return (
    <AuthShell
      title="Reset your password"
      subtitle="We'll email you a link to choose a new one."
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
        <Field label="Email" required htmlFor="email" error={form.formState.errors.email?.message}>
          <Input id="email" type="email" autoComplete="email" {...form.register('email')} />
        </Field>

        {submit.isError && (
          <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2" role="alert">
            <p className="text-sm text-red-700">{submit.error.message}</p>
          </div>
        )}

        <Button type="submit" className="w-full" disabled={submit.isPending}>
          {submit.isPending ? 'Sending…' : 'Send reset link'}
        </Button>
      </form>
    </AuthShell>
  );
}
