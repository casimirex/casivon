import { AlertTriangle, Inbox, RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import type { ApiError } from '@/api/client';

export function EmptyState({
  title,
  message,
  action,
}: {
  title: string;
  message?: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center rounded-md border border-dashed px-6 py-14 text-center">
      <Inbox className="mb-3 h-9 w-9 text-slate-300" />
      <h3 className="text-sm font-semibold text-slate-900">{title}</h3>
      {message && <p className="mt-1 max-w-sm text-sm text-slate-500">{message}</p>}
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}

/**
 * Renders whatever the API said went wrong. A 403 is a different story from a
 * network failure, so they do not get the same message.
 */
export function ErrorState({ error, onRetry }: { error: ApiError | Error; onRetry?: () => void }) {
  const status = (error as ApiError).status;
  const title =
    status === 403
      ? 'You do not have access to this'
      : status === 404
        ? 'Not found'
        : 'Something went wrong';

  return (
    <div className="flex flex-col items-center justify-center rounded-md border border-red-200 bg-red-50 px-6 py-12 text-center">
      <AlertTriangle className="mb-3 h-9 w-9 text-red-400" />
      <h3 className="text-sm font-semibold text-red-900">{title}</h3>
      <p className="mt-1 max-w-md text-sm text-red-700">{error.message}</p>
      {onRetry && status !== 403 && (
        <Button variant="outline" size="sm" className="mt-4 bg-white" onClick={onRetry}>
          <RefreshCw className="mr-2 h-4 w-4" />
          Try again
        </Button>
      )}
    </div>
  );
}
