import { useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { MailWarning, X } from 'lucide-react';
import { http, type ApiError } from '@/api/client';
import { useToast } from '@/components/ui/toast-context';
import { useAuthStore } from '@/store/authStore';

interface ResendResponse {
  message: string;
}

/**
 * Asks the signed-in user to confirm their address.
 *
 * Nothing is gated on verification, so this is a prompt rather than a wall — it
 * can be dismissed for the session and the app carries on regardless. Accounts
 * that predate the feature are unverified too, which is why it has to be
 * possible to put aside rather than block on.
 */
export function VerifyEmailPrompt() {
  const user = useAuthStore((state) => state.user);
  const [dismissed, setDismissed] = useState(false);
  const toast = useToast();

  const resend = useMutation<ResendResponse, ApiError, void>({
    mutationFn: () =>
      http.post<ResendResponse>('/auth/resend-verification', { email: user?.email }),
    // The server answers the same way whether it sent anything or throttled the
    // request, and so does this — showing its message keeps the two consistent.
    onSuccess: (data) => toast.success(data.message),
    onError: (error) => toast.error(error.message),
  });

  if (!user || user.email_verified || dismissed) return null;

  return (
    <div className="flex items-start gap-3 border-b border-amber-200 bg-amber-50 px-4 py-2.5 text-sm sm:px-6">
      <MailWarning className="mt-0.5 h-4 w-4 shrink-0 text-amber-600" aria-hidden />
      <p className="flex-1 text-amber-900">
        <span className="font-medium">{user.email}</span> has not been confirmed yet.{' '}
        <button
          onClick={() => resend.mutate()}
          disabled={resend.isPending}
          className="font-medium underline underline-offset-2 hover:no-underline disabled:opacity-60"
        >
          {resend.isPending ? 'Sending…' : 'Send another link'}
        </button>
      </p>
      <button
        onClick={() => setDismissed(true)}
        aria-label="Dismiss"
        className="rounded p-0.5 text-amber-700 hover:bg-amber-100"
      >
        <X className="h-4 w-4" aria-hidden />
      </button>
    </div>
  );
}
