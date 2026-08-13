import { useState } from 'react';
import { AlertTriangle, CheckCircle2 } from 'lucide-react';
import { PageHeader } from '@/components/common/PageHeader';
import { ErrorState } from '@/components/common/States';
import { TableSkeleton } from '@/components/ui/Skeleton';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/Card';
import { Input } from '@/components/ui/Input';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/Table';
import { useBalanceSheet, useProfitAndLoss, useTrialBalance } from '@/hooks/useAccounting';
import { formatMoney } from '@/lib/utils';
import type { AccountBalance } from '@/types';

type ReportTab = 'trial-balance' | 'profit-and-loss' | 'balance-sheet';

const TABS: Array<{ id: ReportTab; label: string }> = [
  { id: 'trial-balance', label: 'Trial balance' },
  { id: 'profit-and-loss', label: 'Profit & loss' },
  { id: 'balance-sheet', label: 'Balance sheet' },
];

export function Reports() {
  const [tab, setTab] = useState<ReportTab>('trial-balance');
  const [period, setPeriod] = useState<{ date_from?: string; date_to?: string }>({});

  return (
    <div className="space-y-6">
      <PageHeader title="Financial reports" description="Built live from the general ledger" />

      <div className="flex flex-wrap items-end gap-4">
        <div className="flex gap-1 border-b">
          {TABS.map((item) => (
            <button
              key={item.id}
              onClick={() => setTab(item.id)}
              className={`whitespace-nowrap border-b-2 px-4 py-2 text-sm font-medium transition-colors ${
                tab === item.id
                  ? 'border-primary text-primary'
                  : 'border-transparent text-slate-500 hover:text-slate-800'
              }`}
            >
              {item.label}
            </button>
          ))}
        </div>

        <div className="flex items-center gap-2">
          <label className="text-xs text-slate-500" htmlFor="date_from">
            From
          </label>
          <Input
            id="date_from"
            type="date"
            className="w-auto"
            value={period.date_from ?? ''}
            onChange={(event) =>
              setPeriod((current) => ({ ...current, date_from: event.target.value || undefined }))
            }
          />
          <label className="text-xs text-slate-500" htmlFor="date_to">
            To
          </label>
          <Input
            id="date_to"
            type="date"
            className="w-auto"
            value={period.date_to ?? ''}
            onChange={(event) =>
              setPeriod((current) => ({ ...current, date_to: event.target.value || undefined }))
            }
          />
        </div>
      </div>

      {tab === 'trial-balance' && <TrialBalance period={period} />}
      {tab === 'profit-and-loss' && <ProfitAndLoss period={period} />}
      {tab === 'balance-sheet' && <BalanceSheet period={period} />}
    </div>
  );
}

function TrialBalance({ period }: { period: { date_from?: string; date_to?: string } }) {
  const query = useTrialBalance(period);

  if (query.isLoading) return <TableSkeleton columns={5} />;
  if (query.error) return <ErrorState error={query.error} onRetry={query.refetch} />;
  if (!query.data) return null;

  const report = query.data;

  return (
    <div className="space-y-4">
      <BalanceBanner
        balanced={report.is_balanced}
        message={
          report.is_balanced
            ? 'Debits equal credits — the ledger balances.'
            : 'Debits and credits disagree. Recalculate balances or review recent entries.'
        }
      />

      <Card>
        <CardContent className="pt-6">
          <Table>
            <TableHeader>
              <TableRow className="hover:bg-transparent">
                <TableHead>Code</TableHead>
                <TableHead>Account</TableHead>
                <TableHead>Type</TableHead>
                <TableHead className="text-right">Debits</TableHead>
                <TableHead className="text-right">Credits</TableHead>
                <TableHead className="text-right">Balance</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {report.rows.map((row) => (
                <BalanceRow key={row.account_id} row={row} />
              ))}
              <TableRow className="bg-slate-50 font-semibold hover:bg-slate-50">
                <TableCell colSpan={3}>Total</TableCell>
                <TableCell className="text-right tabular-nums">
                  {formatMoney(report.total_debits)}
                </TableCell>
                <TableCell className="text-right tabular-nums">
                  {formatMoney(report.total_credits)}
                </TableCell>
                <TableCell />
              </TableRow>
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}

function ProfitAndLoss({ period }: { period: { date_from?: string; date_to?: string } }) {
  const query = useProfitAndLoss(period);

  if (query.isLoading) return <TableSkeleton columns={3} />;
  if (query.error) return <ErrorState error={query.error} onRetry={query.refetch} />;
  if (!query.data) return null;

  const report = query.data;
  const profitable = Number(report.net_profit) >= 0;

  return (
    <div className="space-y-4">
      <div className="grid gap-4 sm:grid-cols-3">
        <StatCard label="Revenue" value={formatMoney(report.total_revenue)} />
        <StatCard label="Expenses" value={formatMoney(report.total_expenses)} />
        <StatCard
          label="Net profit"
          value={formatMoney(report.net_profit)}
          tone={profitable ? 'text-green-600' : 'text-red-600'}
        />
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <SectionTable title="Revenue" rows={report.revenue} total={report.total_revenue} />
        <SectionTable title="Expenses" rows={report.expenses} total={report.total_expenses} />
      </div>
    </div>
  );
}

function BalanceSheet({ period }: { period: { date_from?: string; date_to?: string } }) {
  const query = useBalanceSheet(period);

  if (query.isLoading) return <TableSkeleton columns={3} />;
  if (query.error) return <ErrorState error={query.error} onRetry={query.refetch} />;
  if (!query.data) return null;

  const report = query.data;

  return (
    <div className="space-y-4">
      <BalanceBanner
        balanced={report.is_balanced}
        message={
          report.is_balanced
            ? 'Assets equal liabilities plus equity.'
            : 'The sheet does not balance. Check for entries posted to the wrong account type.'
        }
      />

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard label="Assets" value={formatMoney(report.total_assets)} />
        <StatCard label="Liabilities" value={formatMoney(report.total_liabilities)} />
        <StatCard label="Equity" value={formatMoney(report.total_equity)} />
        <StatCard label="Retained earnings" value={formatMoney(report.retained_earnings)} />
      </div>

      <div className="grid gap-4 lg:grid-cols-3">
        <SectionTable title="Assets" rows={report.assets} total={report.total_assets} />
        <SectionTable title="Liabilities" rows={report.liabilities} total={report.total_liabilities} />
        <SectionTable title="Equity" rows={report.equity} total={report.total_equity} />
      </div>
    </div>
  );
}

function BalanceRow({ row }: { row: AccountBalance }) {
  return (
    <TableRow>
      <TableCell className="font-mono text-xs text-slate-500">{row.account_code}</TableCell>
      <TableCell className="font-medium">{row.account_name}</TableCell>
      <TableCell className="capitalize text-slate-500">{row.account_type}</TableCell>
      <TableCell className="text-right tabular-nums">{formatMoney(row.total_debits)}</TableCell>
      <TableCell className="text-right tabular-nums">{formatMoney(row.total_credits)}</TableCell>
      <TableCell className="text-right font-medium tabular-nums">{formatMoney(row.balance)}</TableCell>
    </TableRow>
  );
}

function SectionTable({
  title,
  rows,
  total,
}: {
  title: string;
  rows: AccountBalance[];
  total: string;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">{title}</CardTitle>
      </CardHeader>
      <CardContent>
        {rows.length === 0 ? (
          <p className="py-6 text-center text-sm text-slate-400">Nothing posted in this period.</p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow className="hover:bg-transparent">
                <TableHead>Account</TableHead>
                <TableHead className="text-right">Balance</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((row) => (
                <TableRow key={row.account_id}>
                  <TableCell>
                    <span className="font-mono text-xs text-slate-500">{row.account_code}</span>{' '}
                    {row.account_name}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">{formatMoney(row.balance)}</TableCell>
                </TableRow>
              ))}
              <TableRow className="bg-slate-50 font-semibold hover:bg-slate-50">
                <TableCell>Total</TableCell>
                <TableCell className="text-right tabular-nums">{formatMoney(total)}</TableCell>
              </TableRow>
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}

function StatCard({ label, value, tone }: { label: string; value: string; tone?: string }) {
  return (
    <Card>
      <CardContent className="pt-6">
        <p className="text-xs font-medium uppercase tracking-wide text-slate-500">{label}</p>
        <p className={`mt-1 text-2xl font-bold ${tone ?? 'text-slate-900'}`}>{value}</p>
      </CardContent>
    </Card>
  );
}

function BalanceBanner({ balanced, message }: { balanced: boolean; message: string }) {
  return (
    <div
      className={`flex items-start gap-3 rounded-md border p-4 ${
        balanced ? 'border-green-200 bg-green-50' : 'border-amber-200 bg-amber-50'
      }`}
    >
      {balanced ? (
        <CheckCircle2 className="mt-0.5 h-5 w-5 shrink-0 text-green-500" />
      ) : (
        <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-amber-500" />
      )}
      <p className={`text-sm ${balanced ? 'text-green-900' : 'text-amber-900'}`}>{message}</p>
    </div>
  );
}
