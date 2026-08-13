import { useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { useMutation } from '@tanstack/react-query';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Field } from '@/components/common/Field';
import { AuthShell } from '@/components/common/AuthShell';
import { useAuthStore } from '@/store/authStore';
import { http, type ApiError } from '@/api/client';
import { loginSchema, registerSchema, type LoginForm, type RegisterForm } from '@/schemas';
import type { AuthResponse } from '@/types';

export function Login() {
  const navigate = useNavigate();
  const setAuth = useAuthStore((state) => state.setAuth);
  const [mode, setMode] = useState<'login' | 'register'>('login');
  const [serverError, setServerError] = useState<string | null>(null);

  const isLogin = mode === 'login';

  const form = useForm<LoginForm & Partial<RegisterForm>>({
    resolver: zodResolver(isLogin ? loginSchema : registerSchema),
    defaultValues: { email: '', password: '', first_name: '', last_name: '' },
  });

  const submit = useMutation<AuthResponse, ApiError, LoginForm & Partial<RegisterForm>>({
    mutationFn: (values) =>
      http.post<AuthResponse>(isLogin ? '/auth/login' : '/auth/register', values),
    onSuccess: (data) => {
      setAuth(data.user, data.access_token, data.refresh_token);
      navigate('/', { replace: true });
    },
    // Credential failures belong next to the form, not in a toast that flies away.
    onError: (error) => setServerError(error.message),
  });

  const switchMode = () => {
    setMode(isLogin ? 'register' : 'login');
    setServerError(null);
    form.reset();
  };

  return (
    <AuthShell
      title={isLogin ? 'Sign in' : 'Create your account'}
      subtitle={isLogin ? undefined : 'The first account created becomes the administrator.'}
      footer={
        <>
          {isLogin ? "Don't have an account? " : 'Already have an account? '}
          <button
            type="button"
            onClick={switchMode}
            className="font-medium text-primary hover:underline"
          >
            {isLogin ? 'Sign up' : 'Sign in'}
          </button>
        </>
      }
    >
      <form
        onSubmit={form.handleSubmit((values) => {
          setServerError(null);
          submit.mutate(values);
        })}
        className="space-y-4"
        noValidate
      >
        {!isLogin && (
          <div className="grid gap-4 sm:grid-cols-2">
            <Field
              label="First name"
              required
              htmlFor="first_name"
              error={form.formState.errors.first_name?.message}
            >
              <Input id="first_name" autoComplete="given-name" {...form.register('first_name')} />
            </Field>
            <Field
              label="Last name"
              required
              htmlFor="last_name"
              error={form.formState.errors.last_name?.message}
            >
              <Input id="last_name" autoComplete="family-name" {...form.register('last_name')} />
            </Field>
          </div>
        )}

        <Field label="Email" required htmlFor="email" error={form.formState.errors.email?.message}>
          <Input id="email" type="email" autoComplete="email" {...form.register('email')} />
        </Field>

        <Field
          label="Password"
          required
          htmlFor="password"
          error={form.formState.errors.password?.message}
          hint={isLogin ? undefined : 'At least 8 characters'}
        >
          <Input
            id="password"
            type="password"
            autoComplete={isLogin ? 'current-password' : 'new-password'}
            {...form.register('password')}
          />
        </Field>

        {isLogin && (
          <div className="text-right">
            <Link to="/forgot-password" className="text-sm text-slate-600 hover:underline">
              Forgot password?
            </Link>
          </div>
        )}

        {serverError && (
          <div className="rounded-md border border-red-200 bg-red-50 px-3 py-2" role="alert">
            <p className="text-sm text-red-700">{serverError}</p>
          </div>
        )}

        <Button type="submit" className="w-full" disabled={submit.isPending}>
          {submit.isPending ? 'Please wait…' : isLogin ? 'Sign in' : 'Create account'}
        </Button>
      </form>
    </AuthShell>
  );
}
