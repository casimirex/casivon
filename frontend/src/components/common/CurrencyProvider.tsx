import { useEffect } from 'react';
import { useOrganization } from '@/hooks/useSettings';
import { setBaseCurrency } from '@/lib/utils';

/**
 * Feeds the organisation's currency to `formatMoney`.
 *
 * Renders nothing. It exists because `formatMoney` is a plain function used far
 * outside React's reach — table column renderers, report builders, PDF-ish
 * document views — so the value has to live in a module rather than in context.
 * Mounted once inside the authenticated layout, since the profile needs a token
 * to read.
 */
export function CurrencyProvider() {
  const organization = useOrganization();
  const code = organization.data?.default_currency;

  useEffect(() => {
    if (code) setBaseCurrency(code);
  }, [code]);

  return null;
}
