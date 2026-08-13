import * as React from 'react';
import { cn, humanize } from '@/lib/utils';

export type BadgeTone = 'neutral' | 'info' | 'success' | 'warning' | 'danger' | 'muted';

const tones: Record<BadgeTone, string> = {
  neutral: 'bg-slate-100 text-slate-700 ring-slate-200',
  info: 'bg-blue-50 text-blue-700 ring-blue-200',
  success: 'bg-green-50 text-green-700 ring-green-200',
  warning: 'bg-amber-50 text-amber-800 ring-amber-200',
  danger: 'bg-red-50 text-red-700 ring-red-200',
  muted: 'bg-slate-50 text-slate-500 ring-slate-200',
};

export interface BadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  tone?: BadgeTone;
}

export function Badge({ className, tone = 'neutral', ...props }: BadgeProps) {
  return (
    <span
      className={cn(
        'inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium ring-1 ring-inset whitespace-nowrap',
        tones[tone],
        className
      )}
      {...props}
    />
  );
}

/**
 * Maps every workflow status in the system to a colour, so a `draft` quote and a
 * `draft` expense report read the same way everywhere in the UI.
 */
const STATUS_TONES: Record<string, BadgeTone> = {
  // shared
  draft: 'muted',
  active: 'success',
  inactive: 'muted',
  cancelled: 'danger',
  rejected: 'danger',
  // sales
  sent: 'info',
  accepted: 'success',
  expired: 'warning',
  confirmed: 'info',
  processing: 'info',
  partially_shipped: 'warning',
  shipped: 'info',
  delivered: 'success',
  paid: 'success',
  overdue: 'danger',
  // purchasing
  partially_received: 'warning',
  fully_received: 'success',
  closed: 'muted',
  received: 'success',
  // crm
  lead: 'neutral',
  prospect: 'info',
  customer: 'success',
  supplier: 'warning',
  prospecting: 'neutral',
  qualification: 'info',
  proposal: 'info',
  negotiation: 'warning',
  closed_won: 'success',
  closed_lost: 'danger',
  // hr
  pending: 'warning',
  approved: 'success',
  submitted: 'info',
  reimbursed: 'success',
  on_leave: 'warning',
  terminated: 'danger',
  // projects
  planning: 'neutral',
  on_hold: 'warning',
  completed: 'success',
  todo: 'neutral',
  in_progress: 'info',
  review: 'warning',
  done: 'success',
  // priorities
  low: 'muted',
  medium: 'neutral',
  high: 'warning',
  urgent: 'danger',
};

export function StatusBadge({ status, className }: { status: string | null | undefined; className?: string }) {
  if (!status) return <span className="text-muted-foreground">—</span>;
  return (
    <Badge tone={STATUS_TONES[status] ?? 'neutral'} className={className}>
      {humanize(status)}
    </Badge>
  );
}
