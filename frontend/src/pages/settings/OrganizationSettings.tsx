import { useEffect } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { PageHeader } from '@/components/common/PageHeader';
import { Field, FormGrid } from '@/components/common/Field';
import { ErrorState } from '@/components/common/States';
import { DetailSkeleton } from '@/components/ui/Skeleton';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card';
import { useOrganization, useUpdateOrganization } from '@/hooks/useSettings';
import { useWarehouseOptions } from '@/hooks/useInventory';
import { organizationSchema, type OrganizationForm } from '@/schemas';

/** The API returns `null` for an unset field; a text input needs `''`. */
const text = (value: string | null | undefined) => value ?? '';

export function OrganizationSettings() {
  const organization = useOrganization();
  const update = useUpdateOrganization();
  const { options: warehouseOptions } = useWarehouseOptions();

  const form = useForm<OrganizationForm>({
    resolver: zodResolver(organizationSchema),
    defaultValues: {
      name: '',
      legal_name: '',
      email: '',
      phone: '',
      website: '',
      tax_number: '',
      address_line1: '',
      address_line2: '',
      city: '',
      postal_code: '',
      country: '',
      default_currency: 'USD',
      default_dispatch_warehouse_id: '',
    },
  });

  const { reset } = form;
  const loaded = organization.data;

  useEffect(() => {
    if (!loaded) return;
    reset({
      name: loaded.name,
      legal_name: text(loaded.legal_name),
      email: text(loaded.email),
      phone: text(loaded.phone),
      website: text(loaded.website),
      tax_number: text(loaded.tax_number),
      address_line1: text(loaded.address_line1),
      address_line2: text(loaded.address_line2),
      city: text(loaded.city),
      postal_code: text(loaded.postal_code),
      country: text(loaded.country),
      default_currency: loaded.default_currency,
      default_dispatch_warehouse_id: text(loaded.default_dispatch_warehouse_id),
    });
  }, [loaded, reset]);

  if (organization.isLoading) return <DetailSkeleton />;
  if (organization.error) {
    return <ErrorState error={organization.error} onRetry={organization.refetch} />;
  }

  return (
    <div className="space-y-6">
      <PageHeader
        title="Company"
        description="These details identify your business on quotes, invoices and purchase orders."
      />

      <form
        // Submitted whole, blanks included: the API reads an empty string as
        // "clear this field", so `toPayload` — which drops blanks — is wrong here.
        onSubmit={form.handleSubmit((values) => update.mutate(values))}
        className="space-y-6"
        noValidate
      >
        <Card>
          <CardHeader>
            <CardTitle>Identity</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <FormGrid>
              <Field
                label="Company name"
                required
                htmlFor="name"
                error={form.formState.errors.name?.message}
              >
                <Input id="name" {...form.register('name')} />
              </Field>
              <Field
                label="Legal name"
                htmlFor="legal_name"
                hint="If it differs from the trading name"
                error={form.formState.errors.legal_name?.message}
              >
                <Input id="legal_name" {...form.register('legal_name')} />
              </Field>
            </FormGrid>
            <FormGrid>
              <Field
                label="Tax number"
                htmlFor="tax_number"
                error={form.formState.errors.tax_number?.message}
              >
                <Input id="tax_number" {...form.register('tax_number')} />
              </Field>
              <Field
                label="Default currency"
                required
                htmlFor="default_currency"
                // Amounts are stored as entered and never converted, so this is
                // the currency of the whole installation, not a display
                // preference — and it is fixed once anything has been raised.
                hint="Three-letter code. Applies to every document; cannot be changed once quotes, invoices or ledger entries exist."
                error={form.formState.errors.default_currency?.message}
              >
                <Input id="default_currency" maxLength={3} {...form.register('default_currency')} />
              </Field>
              <Field
                label="Dispatch warehouse"
                htmlFor="default_dispatch_warehouse_id"
                // Worth spelling out, because choosing one switches on a refusal
                // as well as a convenience.
                hint="Where goods leave from when an invoice is issued. Leave empty and invoicing moves no stock; choose one and an invoice the shelf cannot cover will be refused."
                error={form.formState.errors.default_dispatch_warehouse_id?.message}
              >
                <Select
                  id="default_dispatch_warehouse_id"
                  options={[
                    { value: '', label: 'Do not ship automatically' },
                    ...warehouseOptions,
                  ]}
                  {...form.register('default_dispatch_warehouse_id')}
                />
              </Field>
            </FormGrid>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Contact</CardTitle>
          </CardHeader>
          <CardContent>
            <FormGrid>
              <Field label="Email" htmlFor="email" error={form.formState.errors.email?.message}>
                <Input id="email" type="email" {...form.register('email')} />
              </Field>
              <Field label="Phone" htmlFor="phone" error={form.formState.errors.phone?.message}>
                <Input id="phone" {...form.register('phone')} />
              </Field>
              <Field
                label="Website"
                htmlFor="website"
                hint="Including https://"
                error={form.formState.errors.website?.message}
              >
                <Input id="website" {...form.register('website')} />
              </Field>
            </FormGrid>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Address</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <Field
              label="Address line 1"
              htmlFor="address_line1"
              error={form.formState.errors.address_line1?.message}
            >
              <Input id="address_line1" {...form.register('address_line1')} />
            </Field>
            <Field
              label="Address line 2"
              htmlFor="address_line2"
              error={form.formState.errors.address_line2?.message}
            >
              <Input id="address_line2" {...form.register('address_line2')} />
            </Field>
            <FormGrid>
              <Field label="City" htmlFor="city" error={form.formState.errors.city?.message}>
                <Input id="city" {...form.register('city')} />
              </Field>
              <Field
                label="Postal code"
                htmlFor="postal_code"
                error={form.formState.errors.postal_code?.message}
              >
                <Input id="postal_code" {...form.register('postal_code')} />
              </Field>
              <Field
                label="Country"
                htmlFor="country"
                error={form.formState.errors.country?.message}
              >
                <Input id="country" {...form.register('country')} />
              </Field>
            </FormGrid>
          </CardContent>
        </Card>

        <div className="flex justify-end">
          <Button type="submit" disabled={update.isPending || !form.formState.isDirty}>
            {update.isPending ? 'Saving…' : 'Save changes'}
          </Button>
        </div>
      </form>
    </div>
  );
}
