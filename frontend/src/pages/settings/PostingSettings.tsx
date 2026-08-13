import { useEffect } from 'react';
import { useForm } from 'react-hook-form';
import { AlertTriangle, CheckCircle2 } from 'lucide-react';
import { PageHeader } from '@/components/common/PageHeader';
import { Field, FormGrid } from '@/components/common/Field';
import { ErrorState } from '@/components/common/States';
import { DetailSkeleton } from '@/components/ui/Skeleton';
import { Button } from '@/components/ui/Button';
import { Select } from '@/components/ui/Select';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/Table';
import {
  accounts as accountsResource,
  useInventoryOpening,
  usePostingAccounts,
  usePostInventoryOpening,
  usePostUnposted,
  useUnpostedDocuments,
  useUpdatePostingAccounts,
} from '@/hooks/useAccounting';
import { formatDate, formatMoney } from '@/lib/utils';
import type { Account, PostingAccounts } from '@/types';

/**
 * A posting role, the account type it requires, and why it exists.
 *
 * Mirrors `POSTING_ROLES` in the backend. The account type is shown rather than
 * merely enforced, because the server refuses a wrong-typed account and finding
 * that out by trial and error is a poor way to fill in a form.
 */
type Role = {
  key: keyof PostingAccounts;
  label: string;
  accountType: Account['account_type'];
  hint: string;
};

/// Grouped by the cycle each role serves. Ten fields in one flat list is a form
/// nobody reads; grouped, each section is a story — what a sale does, what a
/// purchase does, what a claim does.
const GROUPS: Array<{
  title: string;
  description: string;
  roles: Role[];
  /** Posting works without it; it turns an extra behaviour on. */
  optional?: boolean;
}> = [
  {
    title: 'Shared',
    description: 'Used by every cycle.',
    roles: [
      {
        key: 'bank_account_id',
        label: 'Bank',
        accountType: 'asset',
        hint: 'Debited when money arrives, credited when it leaves',
      },
      {
        key: 'fx_gain_loss_account_id',
        label: 'Foreign exchange gain/loss',
        accountType: 'revenue',
        hint: 'Takes the difference when a payment settles at a different rate than the document was raised at',
      },
    ],
  },
  {
    title: 'Selling',
    description: 'What issuing and settling an invoice does.',
    roles: [
      {
        key: 'ar_account_id',
        label: 'Accounts receivable',
        accountType: 'asset',
        hint: 'Debited when an invoice is issued; cleared when it is paid',
      },
      {
        key: 'sales_revenue_account_id',
        label: 'Sales revenue',
        accountType: 'revenue',
        hint: 'Credited with the invoice total less tax',
      },
      {
        key: 'tax_payable_account_id',
        label: 'Tax payable',
        accountType: 'liability',
        hint: 'Credited with the tax you have collected and owe on',
      },
    ],
  },
  {
    title: 'Buying',
    description: 'What receiving goods and paying a supplier does.',
    roles: [
      {
        key: 'accounts_payable_account_id',
        label: 'Accounts payable',
        accountType: 'liability',
        hint: 'Credited when goods arrive; cleared when the supplier is paid',
      },
      {
        key: 'cost_of_sales_account_id',
        label: 'Cost of sales',
        accountType: 'expense',
        hint: 'Debited with the cost of goods — on arrival, or when they are sold if stock is tracked below',
      },
      {
        key: 'purchase_tax_account_id',
        label: 'Purchase tax',
        accountType: 'asset',
        hint: 'Input tax you can reclaim, kept out of the cost',
      },
    ],
  },
  {
    title: 'Expenses',
    description: 'What approving and reimbursing a claim does.',
    roles: [
      {
        key: 'employee_expense_account_id',
        label: 'Employee expense',
        accountType: 'expense',
        hint: 'Debited when a claim is approved',
      },
      {
        key: 'employee_payable_account_id',
        label: 'Employee payable',
        accountType: 'liability',
        hint: 'What you owe staff, kept apart from what you owe suppliers',
      },
    ],
  },
  {
    title: 'Stock',
    description:
      'Optional. Choose both and goods become an asset when they arrive and a cost when they leave, instead of a cost the day they turn up. Leave them empty and nothing changes.',
    optional: true,
    roles: [
      {
        key: 'inventory_account_id',
        label: 'Inventory',
        accountType: 'asset',
        hint: 'What the stock on your shelves is worth, at its moving average cost',
      },
      {
        key: 'inventory_adjustment_account_id',
        label: 'Inventory adjustment',
        accountType: 'expense',
        hint: 'Where a stock-take shortfall or a hand-made correction lands, kept apart from cost of sales so shrinkage stays visible',
      },
    ],
  },
];

const ROLES: Role[] = GROUPS.flatMap((group) => group.roles);

/// Matches the `reference_type` the backend puts on each entry.
const DOCUMENT_LABELS: Record<string, string> = {
  sales_invoice: 'Invoice',
  sales_payment: 'Payment received',
  goods_receipt: 'Goods receipt',
  vendor_payment: 'Payment made',
  expense_report: 'Expense report',
};

const EMPTY: PostingAccounts = Object.fromEntries(
  ROLES.map((role) => [role.key, null])
) as PostingAccounts;

export function PostingSettings() {
  const config = usePostingAccounts();
  const update = useUpdatePostingAccounts();
  const unposted = useUnpostedDocuments();
  const postAll = usePostUnposted();

  // Unpaginated: a chart of accounts is small, and the picker needs all of it.
  const accounts = accountsResource.useList({ page: 1, per_page: 200 });

  const form = useForm<PostingAccounts>({ defaultValues: EMPTY });
  const { reset } = form;
  const loaded = config.data?.accounts;

  useEffect(() => {
    if (loaded) reset(loaded);
  }, [loaded, reset]);

  if (config.isLoading) return <DetailSkeleton />;
  if (config.isError) return <ErrorState error={config.error} onRetry={() => config.refetch()} />;

  const enabled = config.data?.posting_enabled ?? false;
  const outstanding = unposted.data?.documents ?? [];

  /** Only accounts of the right type, so the picker cannot offer a refusal. */
  const optionsFor = (accountType: string) => [
    { value: '', label: 'Not set' },
    ...(accounts.data?.data ?? [])
      .filter((account) => account.account_type === accountType && account.is_active)
      .map((account) => ({
        value: account.id,
        label: `${account.account_code} — ${account.account_name}`,
      })),
  ];

  const onSubmit = form.handleSubmit((values) =>
    update.mutateAsync(
      // An untouched select sends '', which means "not set" rather than an id.
      Object.fromEntries(
        Object.entries(values).map(([key, value]) => [key, value === '' ? null : value])
      ) as PostingAccounts
    )
  );

  return (
    <div className="space-y-6">
      <PageHeader
        title="Automatic posting"
        description="Which account each document posts to. Until every role is chosen, nothing posts at all — not sales, not purchases, not expenses."
      />

      <div
        className={
          enabled
            ? 'flex items-start gap-3 rounded-md border border-emerald-200 bg-emerald-50 px-4 py-3'
            : 'flex items-start gap-3 rounded-md border border-amber-200 bg-amber-50 px-4 py-3'
        }
      >
        {enabled ? (
          <CheckCircle2 className="mt-0.5 h-5 w-5 shrink-0 text-emerald-600" aria-hidden />
        ) : (
          <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-amber-600" aria-hidden />
        )}
        <div className="text-sm">
          {enabled ? (
            <p className="font-medium text-emerald-900">
              Posting is on. Invoices book revenue, goods receipts book cost, and approved expense
              claims book what you owe your staff.
            </p>
          ) : (
            <>
              <p className="font-medium text-amber-900">Posting is off.</p>
              <p className="mt-0.5 text-amber-800">
                Still to choose: {config.data?.missing_roles.join(', ')}. Everything works
                normally meanwhile — it simply does not reach the ledger.
              </p>
            </>
          )}
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Posting accounts</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={onSubmit} className="space-y-6">
            {GROUPS.map((group) => (
              <section key={group.title} className="space-y-3">
                <div>
                  <h3 className="text-sm font-semibold text-slate-900">
                    {group.title}
                    {group.optional && (
                      <span className="ml-2 rounded bg-slate-100 px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide text-slate-500">
                        Optional
                      </span>
                    )}
                  </h3>
                  <p className="text-xs text-slate-500">{group.description}</p>
                </div>
                <FormGrid>
                  {group.roles.map((role) => (
                    <Field key={role.key} label={role.label} hint={role.hint}>
                      {/* `Field` renders the label but does not associate it
                          with the control, so without this the select has no
                          accessible name — ten unlabelled dropdowns to a screen
                          reader. */}
                      <Select
                        options={optionsFor(role.accountType)}
                        aria-label={role.label}
                        {...form.register(role.key)}
                      />
                    </Field>
                  ))}
                </FormGrid>
              </section>
            ))}

            <p className="text-xs text-slate-500">
              Every posting is made in the organisation&rsquo;s base currency, so each account has
              to be denominated in it. Changing an account here affects future postings only —
              entries already written keep the accounts they were posted to.
            </p>

            <div className="flex justify-end">
              <Button type="submit" disabled={update.isPending}>
                {update.isPending ? 'Saving…' : 'Save'}
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>

      <InventoryOpeningCard />

      <Card>
        <CardHeader>
          <CardTitle>Outstanding documents</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {outstanding.length === 0 ? (
            <p className="py-6 text-center text-sm text-slate-400">
              Nothing is waiting to be posted.
            </p>
          ) : (
            <>
              <p className="text-sm text-slate-600">
                These have been issued or settled but have no entries against them — either they
                predate automatic posting, or a posting did not complete. Posting them is safe to
                run more than once.
              </p>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Document</TableHead>
                    <TableHead>Reference</TableHead>
                    <TableHead>Date</TableHead>
                    <TableHead className="text-right">Amount</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {outstanding.map((document) => (
                    <TableRow key={`${document.kind}-${document.id}`}>
                      <TableCell>{DOCUMENT_LABELS[document.kind] ?? document.kind}</TableCell>
                      <TableCell className="font-medium">{document.reference}</TableCell>
                      <TableCell>{formatDate(document.date)}</TableCell>
                      <TableCell className="text-right tabular-nums">
                        {formatMoney(document.base_amount)}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
              <div className="flex justify-end">
                <Button onClick={() => postAll.mutate()} disabled={!enabled || postAll.isPending}>
                  {postAll.isPending ? 'Posting…' : `Post ${outstanding.length} document(s)`}
                </Button>
              </div>
              {!enabled && (
                <p className="text-right text-xs text-slate-500">
                  Choose the posting accounts first — there is nowhere to post to yet.
                </p>
              )}
            </>
          )}
        </CardContent>
      </Card>
    </div>
  );
}


/**
 * Opening the Inventory account when an installation switches to perpetual
 * costing.
 *
 * Stock already on the shelves was expensed the day it arrived, so selling it
 * under the new rules would credit an Inventory account that was never debited
 * and drive an asset negative. This is the one-time entry that squares that —
 * shown before it is posted, because the figure deserves a look.
 */
function InventoryOpeningCard() {
  const opening = useInventoryOpening();
  const post = usePostInventoryOpening();

  // Nothing to say until the accounts are mapped: there would be nowhere to
  // post to, and the card would be a question nobody asked.
  if (!opening.data?.perpetual_inventory) return null;

  const { already_posted, total_value, lines, assumes_everything_was_received } = opening.data;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Opening stock balance</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {already_posted ? (
          <p className="text-sm text-slate-600">
            Posted. Stock on hand is on the balance sheet, and goods now become a cost when they
            leave rather than when they arrive.
          </p>
        ) : (
          <>
            <p className="text-sm text-slate-600">
              Stock you already hold was charged to cost of sales when it arrived. This posts it to
              the Inventory account and relieves that cost, so the balance sheet shows what is on
              the shelves.
            </p>
            <p className="text-xs text-slate-500">{assumes_everything_was_received}</p>
          </>
        )}

        {lines.length === 0 ? (
          <p className="py-6 text-center text-sm text-slate-400">There is no stock on hand.</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>SKU</TableHead>
                <TableHead>Product</TableHead>
                <TableHead className="text-right">On hand</TableHead>
                <TableHead className="text-right">Average cost</TableHead>
                <TableHead className="text-right">Value</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {lines.map((line) => (
                <TableRow key={line.product_id}>
                  <TableCell className="font-medium">{line.sku}</TableCell>
                  <TableCell>{line.name}</TableCell>
                  <TableCell className="text-right tabular-nums">{line.quantity}</TableCell>
                  <TableCell className="text-right tabular-nums">
                    {line.average_cost ?? '—'}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {formatMoney(line.value)}
                  </TableCell>
                </TableRow>
              ))}
              <TableRow className="bg-slate-50 font-semibold hover:bg-slate-50">
                <TableCell colSpan={4}>Total</TableCell>
                <TableCell className="text-right tabular-nums">
                  {formatMoney(total_value)}
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        )}

        {!already_posted && lines.length > 0 && (
          <div className="flex justify-end">
            <Button onClick={() => post.mutate()} disabled={post.isPending}>
              {post.isPending ? 'Posting…' : `Post ${formatMoney(total_value)} to Inventory`}
            </Button>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
