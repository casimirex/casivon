import { useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { ArrowRight, Pencil, Trash2 } from 'lucide-react';
import { PageHeader } from '@/components/common/PageHeader';
import { DocumentLines, SummaryGrid } from '@/components/common/DocumentView';
import { DetailSkeleton } from '@/components/ui/Skeleton';
import { ErrorState } from '@/components/common/States';
import { Button } from '@/components/ui/Button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card';
import { StatusBadge } from '@/components/ui/Badge';
import { ConfirmDialog } from '@/components/ui/Dialog';
import { quotes, useConvertQuoteToOrder, useQuoteStatus } from '@/hooks/useSales';
import { formatDate, formatMoney } from '@/lib/utils';

/** Transitions the server will accept from each quote status. */
const NEXT_STATUS: Record<string, Array<{ status: string; label: string }>> = {
  draft: [
    { status: 'sent', label: 'Send to customer' },
    { status: 'expired', label: 'Mark expired' },
  ],
  sent: [
    { status: 'accepted', label: 'Mark accepted' },
    { status: 'rejected', label: 'Mark rejected' },
    { status: 'expired', label: 'Mark expired' },
  ],
};

export function QuoteDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const query = quotes.useOne(id);
  const setStatus = useQuoteStatus();
  const convert = useConvertQuoteToOrder();
  const remove = quotes.useRemove({
    successMessage: 'Quote deleted',
    onSuccess: () => navigate('/sales/quotes'),
  });
  const [confirmDelete, setConfirmDelete] = useState(false);

  if (query.isLoading) return <DetailSkeleton />;
  if (query.error) return <ErrorState error={query.error} onRetry={query.refetch} />;
  if (!query.data) return null;

  const quote = query.data;
  const transitions = NEXT_STATUS[quote.status] ?? [];
  const isDraft = quote.status === 'draft';
  const canConvert = quote.status === 'accepted';
  const busy = setStatus.isPending || convert.isPending || remove.isPending;

  return (
    <div className="space-y-6">
      <PageHeader
        title={quote.quote_number}
        backTo="/sales/quotes"
        backLabel="Back to quotes"
        badge={<StatusBadge status={quote.status} />}
        actions={
          <>
            {transitions.map((transition) => (
              <Button
                key={transition.status}
                variant="outline"
                disabled={busy}
                onClick={() => setStatus.mutate({ id: quote.id, status: transition.status })}
              >
                {transition.label}
              </Button>
            ))}

            {canConvert && (
              <Button
                disabled={busy}
                onClick={() =>
                  convert.mutate(
                    { id: quote.id },
                    { onSuccess: (order) => navigate(`/sales/orders/${order.id}`) }
                  )
                }
              >
                Convert to order
                <ArrowRight className="ml-1 h-4 w-4" />
              </Button>
            )}

            {isDraft && (
              <>
                <Button variant="outline" onClick={() => navigate(`/sales/quotes/${quote.id}/edit`)}>
                  <Pencil className="mr-1 h-4 w-4" />
                  Edit
                </Button>
                <Button variant="destructive" onClick={() => setConfirmDelete(true)} disabled={busy}>
                  <Trash2 className="mr-1 h-4 w-4" />
                  Delete
                </Button>
              </>
            )}
          </>
        }
      />

      <SummaryGrid
        items={[
          { label: 'Issued', value: formatDate(quote.issue_date) },
          { label: 'Expires', value: formatDate(quote.expiry_date) },
          { label: 'Total', value: formatMoney(quote.total, quote.currency) },
          { label: 'Currency', value: quote.currency },
        ]}
      />

      <Card>
        <CardHeader>
          <CardTitle className="text-base">Line items</CardTitle>
        </CardHeader>
        <CardContent>
          <DocumentLines
            lines={quote.lines}
            currency={quote.currency}
            subtotal={quote.subtotal}
            tax={quote.tax_amount}
            total={quote.total}
            baseTotal={quote.base_total}
            fxRate={quote.fx_rate}
          />
        </CardContent>
      </Card>

      {(quote.notes || quote.terms) && (
        <Card>
          <CardHeader>
            <CardTitle className="text-base">Notes and terms</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4 text-sm text-slate-600">
            {quote.notes && (
              <div>
                <p className="font-medium text-slate-900">Notes</p>
                <p className="whitespace-pre-wrap">{quote.notes}</p>
              </div>
            )}
            {quote.terms && (
              <div>
                <p className="font-medium text-slate-900">Terms</p>
                <p className="whitespace-pre-wrap">{quote.terms}</p>
              </div>
            )}
          </CardContent>
        </Card>
      )}

      <ConfirmDialog
        open={confirmDelete}
        onClose={() => setConfirmDelete(false)}
        onConfirm={() => remove.mutate(quote.id)}
        title="Delete quote"
        message={`Delete ${quote.quote_number}? Only draft quotes can be deleted.`}
        confirmLabel="Delete quote"
        busy={remove.isPending}
      />
    </div>
  );
}
