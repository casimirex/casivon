import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { http, type ApiError } from '@/api/client';
import { useToast } from '@/components/ui/toast-context';
import { createResource, useAction } from './useResource';
import type {
  Account,
  AccountNode,
  BalanceSheetReport,
  BankAccount,
  GeneralLedgerEntry,
  InventoryOpeningReport,
  PostingAccounts,
  PostingConfiguration,
  PostingRunReport,
  ProfitAndLossReport,
  TaxRate,
  TrialBalanceReport,
  UnpostedReport,
} from '@/types';

export const accounts = createResource<Account>('/accounting/accounts', 'accounts');
export const ledgerEntries = createResource<GeneralLedgerEntry>(
  '/accounting/ledger-entries',
  'ledger-entries'
);
export const bankAccounts = createResource<BankAccount>(
  '/accounting/bank-accounts',
  'bank-accounts'
);
export const taxRates = createResource<TaxRate>('/accounting/tax-rates', 'tax-rates');

export function useAccountTree() {
  return useQuery({
    queryKey: ['accounts', 'tree'],
    queryFn: () => http.get<AccountNode[]>('/accounting/accounts/tree'),
  });
}

export interface ReportPeriod {
  date_from?: string;
  date_to?: string;
  /** Lets a period be passed straight through as query params. */
  [key: string]: string | undefined;
}

export function useTrialBalance(period: ReportPeriod) {
  return useQuery({
    queryKey: ['reports', 'trial-balance', period],
    queryFn: () => http.get<TrialBalanceReport>('/accounting/reports/trial-balance', period),
  });
}

export function useProfitAndLoss(period: ReportPeriod) {
  return useQuery({
    queryKey: ['reports', 'profit-and-loss', period],
    queryFn: () => http.get<ProfitAndLossReport>('/accounting/reports/profit-and-loss', period),
  });
}

export function useBalanceSheet(period: ReportPeriod) {
  return useQuery({
    queryKey: ['reports', 'balance-sheet', period],
    queryFn: () => http.get<BalanceSheetReport>('/accounting/reports/balance-sheet', period),
  });
}

/** Rebuilds every account balance from the ledger; also refreshes the reports. */
export function useRecalculateBalances() {
  return useAction<{ accounts_updated: number }, void>(
    'accounts',
    () => http.post<{ accounts_updated: number }>('/accounting/accounts/recalculate', {}),
    { successMessage: 'Account balances recalculated', invalidateKeys: ['reports'] }
  );
}

export function useAccountOptions(filter?: { account_type?: string }) {
  const { data, isLoading } = accounts.useList({
    per_page: 200,
    is_active: true,
    sort: 'account_code',
    ...filter,
  });
  return {
    isLoading,
    accounts: data?.data ?? [],
    options: (data?.data ?? []).map((account) => ({
      value: account.id,
      label: `${account.account_code} — ${account.account_name}`,
    })),
  };
}

// ------------------------------------------------------- automatic posting

/**
 * Which account each automatic posting uses, and whether posting is on at all.
 *
 * `posting_enabled` is false until all five roles are mapped, and while it is
 * false sales documents post nothing — so this is the switch, not a preference.
 */
export function usePostingAccounts() {
  return useQuery({
    queryKey: ['posting-accounts'],
    queryFn: () => http.get<PostingConfiguration>('/accounting/posting-accounts'),
  });
}

export function useUpdatePostingAccounts() {
  const queryClient = useQueryClient();
  const toast = useToast();

  return useMutation<PostingConfiguration, ApiError, PostingAccounts>({
    mutationFn: (body) => http.put<PostingConfiguration>('/accounting/posting-accounts', body),
    onSuccess: (config) => {
      queryClient.invalidateQueries({ queryKey: ['posting-accounts'] });
      // Completing the mapping is what turns documents from unpostable into
      // merely unposted, so the outstanding list changes meaning with it.
      queryClient.invalidateQueries({ queryKey: ['unposted'] });
      toast.success(
        config.posting_enabled
          ? 'Posting accounts saved. New invoices will post automatically.'
          : 'Saved. Posting stays off until every role has an account.'
      );
    },
    onError: (error) => toast.error(error.message),
  });
}

/** What the ledger is owed. Empty is the healthy state. */
export function useUnpostedDocuments() {
  return useQuery({
    queryKey: ['unposted'],
    queryFn: () => http.get<UnpostedReport>('/accounting/unposted'),
  });
}

export function usePostUnposted() {
  const queryClient = useQueryClient();
  const toast = useToast();

  return useMutation<PostingRunReport, ApiError, void>({
    mutationFn: () => http.post<PostingRunReport>('/accounting/post-unposted', {}),
    onSuccess: (report) => {
      queryClient.invalidateQueries({ queryKey: ['unposted'] });
      // Posting moves balances, so anything reading them is now stale.
      queryClient.invalidateQueries({ queryKey: ['accounts'] });
      queryClient.invalidateQueries({ queryKey: ['ledger-entries'] });
      const total = report.invoices_posted + report.payments_posted;
      toast.success(
        total === 0
          ? 'Nothing was outstanding.'
          : `Posted ${report.invoices_posted} invoice(s) and ${report.payments_posted} payment(s).`
      );
    },
    onError: (error) => toast.error(error.message),
  });
}

/**
 * What switching to perpetual costing would put on the balance sheet.
 *
 * Stock already on the shelves was expensed when it arrived, so selling it under
 * the new rules would credit an Inventory account that was never debited. This
 * is the preview of the one-time entry that squares that.
 */
export function useInventoryOpening() {
  return useQuery({
    queryKey: ['inventory-opening'],
    queryFn: () => http.get<InventoryOpeningReport>('/accounting/inventory-opening'),
  });
}

export function usePostInventoryOpening() {
  const queryClient = useQueryClient();
  const toast = useToast();

  return useMutation<InventoryOpeningReport, ApiError, void>({
    mutationFn: () =>
      http.post<InventoryOpeningReport>('/accounting/inventory-opening', {}),
    onSuccess: (report) => {
      queryClient.invalidateQueries({ queryKey: ['inventory-opening'] });
      // The entry moves two balances, so anything reading them is now stale.
      queryClient.invalidateQueries({ queryKey: ['accounts'] });
      queryClient.invalidateQueries({ queryKey: ['ledger-entries'] });
      toast.success(
        report.already_posted
          ? 'Opening balance posted. Stock now shows on the balance sheet.'
          : 'There was no stock on hand to open with.'
      );
    },
    onError: (error) => toast.error(error.message),
  });
}
