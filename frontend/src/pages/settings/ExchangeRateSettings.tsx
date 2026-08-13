import { useState } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { Trash2 } from 'lucide-react';
import { z } from 'zod';
import { PageHeader } from '@/components/common/PageHeader';
import { Field, FormGrid } from '@/components/common/Field';
import { EmptyState, ErrorState } from '@/components/common/States';
import { TableSkeleton } from '@/components/ui/Skeleton';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { ConfirmDialog } from '@/components/ui/Dialog';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/Table';
import {
  useCurrencies,
  useDeleteFxRate,
  useFxRates,
  useUpsertFxRate,
} from '@/hooks/useSettings';
import { currencyCode } from '@/schemas';
import { formatDate } from '@/lib/utils';
import type { FxRate } from '@/types';

const rateSchema = z.object({
  currency: currencyCode,
  effective_from: z.string().min(1, 'Effective date is required'),
  rate: z.coerce
    .number({ invalid_type_error: 'Enter a rate' })
    .positive('A rate is what an amount is multiplied by, so it has to be above zero'),
});

type RateForm = z.input<typeof rateSchema>;

/** Groups rates under their currency, newest first — the API already sorts them. */
function byCurrency(rates: FxRate[]): [string, FxRate[]][] {
  const groups = new Map<string, FxRate[]>();
  for (const rate of rates) {
    const existing = groups.get(rate.currency);
    if (existing) existing.push(rate);
    else groups.set(rate.currency, [rate]);
  }
  return [...groups.entries()];
}

export function ExchangeRateSettings() {
  const currencies = useCurrencies();
  const rates = useFxRates();
  const upsert = useUpsertFxRate();
  const remove = useDeleteFxRate();
  const [removing, setRemoving] = useState<FxRate | null>(null);

  const base = currencies.data?.base ?? '';

  const form = useForm<RateForm>({
    resolver: zodResolver(rateSchema),
    defaultValues: {
      currency: '',
      effective_from: new Date().toISOString().slice(0, 10),
      rate: '' as unknown as number,
    },
  });

  const onSubmit = form.handleSubmit(async (values) => {
    await upsert.mutateAsync({
      currency: values.currency.toUpperCase(),
      effective_from: values.effective_from,
      // Sent as a string: a rate carries more decimal places than a float
      // round-trips cleanly, and the server stores DECIMAL(18, 8).
      rate: String(values.rate),
    });
    form.reset({
      currency: '',
      effective_from: values.effective_from,
      rate: '' as unknown as number,
    });
  });

  if (rates.isError) return <ErrorState error={rates.error} onRetry={() => rates.refetch()} />;

  const grouped = byCurrency(rates.data ?? []);

  return (
    <div className="space-y-6">
      <PageHeader
        title="Exchange rates"
        description={
          base
            ? `What one unit of each currency is worth in ${base}. Every document is restated at the rate in force on its own date, and keeps it.`
            : 'What one unit of each currency is worth in the base currency.'
        }
      />

      <Card>
        <CardHeader>
          <CardTitle>Add or correct a rate</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={onSubmit} className="space-y-4">
            <FormGrid>
              <Field label="Currency" required error={form.formState.errors.currency?.message}>
                <Input
                  placeholder="EUR"
                  maxLength={3}
                  className="uppercase"
                  {...form.register('currency')}
                />
              </Field>
              <Field
                label="Effective from"
                required
                error={form.formState.errors.effective_from?.message}
                hint="Stays in force until a later rate supersedes it"
              >
                <Input type="date" {...form.register('effective_from')} />
              </Field>
              <Field
                label={base ? `Rate (1 unit = ? ${base})` : 'Rate'}
                required
                error={form.formState.errors.rate?.message}
              >
                <Input type="number" step="0.00000001" min={0} {...form.register('rate')} />
              </Field>
            </FormGrid>

            <p className="text-xs text-slate-500">
              Sending a currency and date that already have a rate corrects it rather than adding a
              second one. {base && `${base} needs no rate: it is worth 1 of itself by definition.`}
            </p>

            <div className="flex justify-end">
              <Button type="submit" disabled={upsert.isPending}>
                {upsert.isPending ? 'Saving…' : 'Save rate'}
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>

      {rates.isLoading ? (
        <TableSkeleton />
      ) : grouped.length === 0 ? (
        <EmptyState
          title="No exchange rates yet"
          message={
            base
              ? `Every document is raised in ${base}. Add a rate to start trading in another currency.`
              : 'Add a rate to start trading in another currency.'
          }
        />
      ) : (
        grouped.map(([currency, history]) => (
          <Card key={currency}>
            <CardHeader>
              <CardTitle>
                {currency}
                {base && <span className="ml-2 text-sm font-normal text-slate-500">→ {base}</span>}
              </CardTitle>
            </CardHeader>
            <CardContent>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Effective from</TableHead>
                    <TableHead>Rate</TableHead>
                    <TableHead className="w-16" />
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {history.map((rate, index) => (
                    <TableRow key={rate.id}>
                      <TableCell>
                        {formatDate(rate.effective_from)}
                        {/* The newest row is what a document raised today gets. */}
                        {index === 0 && (
                          <span className="ml-2 text-xs text-slate-400">current</span>
                        )}
                      </TableCell>
                      <TableCell className="tabular-nums">{rate.rate}</TableCell>
                      <TableCell>
                        <Button
                          variant="ghost"
                          size="sm"
                          aria-label={`Remove the ${currency} rate effective ${rate.effective_from}`}
                          onClick={() => setRemoving(rate)}
                        >
                          <Trash2 className="h-4 w-4" aria-hidden />
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        ))
      )}

      <ConfirmDialog
        open={removing !== null}
        onClose={() => setRemoving(null)}
        busy={remove.isPending}
        title="Remove this exchange rate?"
        message={
          removing
            ? `The ${removing.currency} rate effective ${removing.effective_from} will be removed. Documents already raised keep the amounts they were booked at — but if this is the last rate for ${removing.currency} and documents are denominated in it, the removal will be refused.`
            : ''
        }
        confirmLabel="Remove"
        onConfirm={async () => {
          if (removing) await remove.mutateAsync(removing.id);
          setRemoving(null);
        }}
      />
    </div>
  );
}
