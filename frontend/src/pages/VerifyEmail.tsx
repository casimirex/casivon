import { useEffect, useRef } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { useMutation } from '@tanstack/react-query';
import { CheckCircle2, Loader2, XCircle } from 'lucide-react';
import { AuthShell } from '@/components/common/AuthShell';
import { http, type ApiError } from '@/api/client';
import { useAuthStore } from '@/store/authStore';

interface VerifyEmailResponse {
  email: string;
  email_verified: boolean;
}

/**
 * Spends the token in a verification link.
 *
 * Submits on mount rather than behind a button: the person already expressed
 * intent by clicking the link in their inbox, and asking them to confirm a
 * confirmation is a step with nothing behind it.
 */
export function VerifyEmail() {
  const [params] = useSearchParams();
  const token = params.get('token') ?? '';
  const user = useAuthStore((state) => state.user);
  const setUser = useAuthStore((state) => state.setUser);

  const verify = useMutation<VerifyEmailResponse, ApiError, string>({
    mutationFn: (token) => http.post<VerifyEmailResponse>('/auth/verify-email', { token }),
    onSuccess: (data) => {
      // Someone verifying in the same browser they are signed into should not
      // keep seeing the prompt. Anyone else has no session to update.
      if (user && user.email === data.email) {
        setUser({ ...user, email_verified: true });
      }
    },
  });

  // Ref-guarded so React's development double-invoke does not spend the token
  // twice — the second attempt would fail, and the screen would report a
  // successful verification as broken.
  const submitted = useRef(false);
  const { mutate } = verify;

  useEffect(() => {
    if (!token || submitted.current) return;
    submitted.current = true;
    mutate(token);
  }, [token, mutate]);

  if (!token) {
    return (
      <AuthShell
        title="This link is incomplete"
        footer={
          <Link to="/login" className="font-medium text-primary hover:underline">
            Back to sign in
          </Link>
        }
      >
        <p className="text-sm text-slate-600">
          The verification link is missing its token. It may have been cut short by your email
          client — try opening it again from the original message.
        </p>
      </AuthShell>
    );
  }

  if (verify.isPending) {
    return (
      <AuthShell title="Confirming your address">
        <p className="flex items-center gap-2 text-sm text-slate-600">
          <Loader2 className="h-4 w-4 animate-spin" aria-hidden />
          One moment…
        </p>
      </AuthShell>
    );
  }

  if (verify.isError) {
    return (
      <AuthShell
        title="This link is no longer valid"
        footer={
          <Link to="/login" className="font-medium text-primary hover:underline">
            Back to sign in
          </Link>
        }
      >
        <p className="flex items-start gap-2 text-sm text-slate-600">
          <XCircle className="mt-0.5 h-4 w-4 shrink-0 text-red-500" aria-hidden />
          <span>
            {verify.error.message} Verification links work once and expire; you can ask for a new
            one from the prompt after signing in.
          </span>
        </p>
      </AuthShell>
    );
  }

  return (
    <AuthShell
      title="Address confirmed"
      footer={
        <Link to="/login" className="font-medium text-primary hover:underline">
          Continue to sign in
        </Link>
      }
    >
      <p className="flex items-start gap-2 text-sm text-slate-600">
        <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-emerald-600" aria-hidden />
        <span>
          {verify.data?.email} is confirmed. Nothing else is needed — this was only to check the
          address reaches you.
        </span>
      </p>
    </AuthShell>
  );
}
