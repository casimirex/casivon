import * as React from 'react';
import { Field } from '@/components/common/Field';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { useCurrencies } from '@/hooks/useSettings';
import { getBaseCurrency } from '@/lib/utils';

export interface CurrencyFieldProps extends React.SelectHTMLAttributes<HTMLSelectElement> {
  label?: string;
  error?: string;
}

/**
 * Picks the currency a document is raised in.
 *
 * The list is what the server will actually accept: the organisation's own
 * currency, which needs no rate, plus every currency with an exchange rate on
 * file. Offering anything else would produce a document the API refuses,
 * because an amount it cannot restate is an amount no report can add up.
 *
 * With no rates entered there is exactly one currency, and a dropdown holding a
 * single option is a worse control than a label — so it falls back to the
 * read-only display it had before multi-currency existed, and says where to go
 * to change that. In that state it stays unregistered, so the form submits no
 * currency at all and the server stamps its own; that is the one behaviour that
 * cannot go stale.
 */
export const CurrencyField = React.forwardRef<HTMLSelectElement, CurrencyFieldProps>(
  ({ label = 'Currency', error, ...field }, ref) => {
    const currencies = useCurrencies();
    const base = currencies.data?.base ?? getBaseCurrency();
    const available = currencies.data?.available ?? [];

    if (available.length <= 1) {
      return (
        <Field
          label={label}
          hint="Add exchange rates under Settings → Exchange rates to raise documents in another currency"
        >
          <Input value={base} readOnly disabled aria-label={label} />
        </Field>
      );
    }

    return (
      <Field
        label={label}
        error={error}
        hint={`Restated in ${base} at the rate in force on the document's date`}
      >
        <Select ref={ref} options={available} aria-label={label} {...field} />
      </Field>
    );
  }
);

CurrencyField.displayName = 'CurrencyField';
