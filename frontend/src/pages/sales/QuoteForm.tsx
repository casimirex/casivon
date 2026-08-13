import { useNavigate, useParams } from 'react-router-dom';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { PageHeader } from '@/components/common/PageHeader';
import { Field, FormGrid } from '@/components/common/Field';
import { LineItemsEditor } from '@/components/common/LineItemsEditor';
import { asLineForm } from '@/lib/lineMath';
import { DetailSkeleton } from '@/components/ui/Skeleton';
import { ErrorState } from '@/components/common/States';
import { Button } from '@/components/ui/Button';
import { Card, CardContent } from '@/components/ui/Card';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { Textarea } from '@/components/ui/Textarea';
import { CurrencyField } from '@/components/common/CurrencyField';
import { quotes } from '@/hooks/useSales';
import { useCompanyOptions } from '@/hooks/useCrm';
import { useProductOptions } from '@/hooks/useInventory';
import { toDateInput, getBaseCurrency } from '@/lib/utils';
import { quoteSchema, type QuoteForm as QuoteFormValues } from '@/schemas';

export function QuoteForm() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const isEdit = Boolean(id);

  const detail = quotes.useOne(id);
  const { options: companyOptions } = useCompanyOptions();
  const { options: productOptions } = useProductOptions();

  const create = quotes.useCreate({
    successMessage: 'Quote created',
    onSuccess: (quote) => navigate(`/sales/quotes/${quote.id}`),
  });
  const update = quotes.useUpdate({
    successMessage: 'Quote updated',
    onSuccess: () => navigate(`/sales/quotes/${id}`),
  });

  const form = useForm<QuoteFormValues>({
    resolver: zodResolver(quoteSchema),
    defaultValues: {
      customer_id: '',
      issue_date: toDateInput(),
      // A month's validity is the usual default.
      expiry_date: toDateInput(new Date(Date.now() + 30 * 86_400_000)),
      notes: '',
      terms: '',
      lines: [{ description: '', quantity: 1, unit_price: 0, discount_percent: 0, tax_rate: 0 }],
    },
    // Populated from the server once the quote loads (edit mode only).
    values: detail.data
      ? {
          customer_id: detail.data.customer_id,
          contact_id: detail.data.contact_id ?? '',
          issue_date: detail.data.issue_date,
          expiry_date: detail.data.expiry_date,
          notes: detail.data.notes ?? '',
          terms: detail.data.terms ?? '',
          lines: detail.data.lines.map((line) => ({
            product_id: line.product_id ?? '',
            description: line.description,
            quantity: line.quantity,
            unit_price: Number(line.unit_price),
            discount_percent: Number(line.discount_percent),
            tax_rate: Number(line.tax_rate),
          })),
        }
      : undefined,
  });

  if (isEdit && detail.isLoading) return <DetailSkeleton />;
  if (isEdit && detail.error) return <ErrorState error={detail.error} onRetry={detail.refetch} />;

  const busy = create.isPending || update.isPending;
  const { errors } = form.formState;
  // One currency per installation, so this is not a per-document choice.
  const currency = getBaseCurrency();

  const onSubmit = form.handleSubmit((values) => {
    const body = {
      ...values,
      // Money crosses the wire as strings so nothing is lost to float rounding.
      lines: values.lines.map((line) => ({
        ...line,
        product_id: line.product_id || undefined,
        unit_price: Number(line.unit_price).toFixed(2),
        discount_percent: Number(line.discount_percent ?? 0).toFixed(2),
        tax_rate: Number(line.tax_rate ?? 0).toFixed(2),
        quantity: Number(line.quantity),
      })),
    };

    if (id) update.mutate({ id, body });
    else create.mutate(body);
  });

  return (
    <form onSubmit={onSubmit} className="space-y-6" noValidate>
      <PageHeader
        title={isEdit ? `Edit ${detail.data?.quote_number ?? 'quote'}` : 'New quote'}
        backTo={isEdit ? `/sales/quotes/${id}` : '/sales/quotes'}
        backLabel={isEdit ? 'Back to quote' : 'Back to quotes'}
        actions={
          <>
            <Button
              type="button"
              variant="outline"
              onClick={() => navigate(isEdit ? `/sales/quotes/${id}` : '/sales/quotes')}
              disabled={busy}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={busy}>
              {busy ? 'Saving…' : isEdit ? 'Save changes' : 'Create quote'}
            </Button>
          </>
        }
      />

      <Card>
        <CardContent className="space-y-4 pt-6">
          <FormGrid>
            <Field label="Customer" required error={errors.customer_id?.message}>
              <Select
                options={companyOptions}
                placeholder="Select a customer"
                {...form.register('customer_id')}
              />
            </Field>
            <CurrencyField {...form.register('currency')} />
            <Field label="Issue date" required error={errors.issue_date?.message}>
              <Input type="date" {...form.register('issue_date')} />
            </Field>
            <Field label="Expiry date" required error={errors.expiry_date?.message}>
              <Input type="date" {...form.register('expiry_date')} />
            </Field>
          </FormGrid>
        </CardContent>
      </Card>

      <Card>
        <CardContent className="pt-6">
          <LineItemsEditor
            form={asLineForm(form)}
            productOptions={productOptions}
            currency={currency}
            disabled={busy}
          />
        </CardContent>
      </Card>

      <Card>
        <CardContent className="space-y-4 pt-6">
          <Field label="Notes" error={errors.notes?.message}>
            <Textarea {...form.register('notes')} />
          </Field>
          <Field label="Terms" error={errors.terms?.message}>
            <Textarea {...form.register('terms')} />
          </Field>
        </CardContent>
      </Card>
    </form>
  );
}
