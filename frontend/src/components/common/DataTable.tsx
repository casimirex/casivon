import * as React from 'react';
import { ArrowDown, ArrowUp, ChevronLeft, ChevronRight, ChevronsUpDown } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/Button';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/Table';
import { TableSkeleton } from '@/components/ui/Skeleton';
import { EmptyState, ErrorState } from './States';
import type { ApiError } from '@/api/client';
import type { PaginationMeta } from '@/types';

export interface Column<T> {
  /** Stable key; also the sort field sent to the API when `sortable`. */
  key: string;
  header: string;
  render: (row: T) => React.ReactNode;
  /** Set when the backend's sort allow-list accepts this column. */
  sortable?: boolean;
  className?: string;
  align?: 'left' | 'right';
}

export interface DataTableProps<T> {
  columns: Column<T>[];
  rows: T[] | undefined;
  rowKey: (row: T) => string;
  isLoading?: boolean;
  error?: ApiError | Error | null;
  onRetry?: () => void;
  onRowClick?: (row: T) => void;
  pagination?: PaginationMeta;
  onPageChange?: (page: number) => void;
  /** Current `?sort=` value, e.g. `-created_at`. */
  sort?: string;
  onSortChange?: (sort: string) => void;
  emptyTitle?: string;
  emptyMessage?: string;
  emptyAction?: React.ReactNode;
}

export function DataTable<T>({
  columns,
  rows,
  rowKey,
  isLoading,
  error,
  onRetry,
  onRowClick,
  pagination,
  onPageChange,
  sort,
  onSortChange,
  emptyTitle = 'Nothing here yet',
  emptyMessage,
  emptyAction,
}: DataTableProps<T>) {
  if (error) return <ErrorState error={error} onRetry={onRetry} />;
  if (isLoading) return <TableSkeleton columns={columns.length} />;
  if (!rows?.length) {
    return <EmptyState title={emptyTitle} message={emptyMessage} action={emptyAction} />;
  }

  // `-field` means descending; clicking a sorted column flips it.
  const activeField = sort?.replace(/^-/, '');
  const isDescending = sort?.startsWith('-') ?? false;

  const toggleSort = (key: string) => {
    if (!onSortChange) return;
    onSortChange(activeField === key && !isDescending ? `-${key}` : key);
  };

  return (
    <div className="space-y-4">
      <Table>
        <TableHeader>
          <TableRow className="hover:bg-transparent">
            {columns.map((column) => (
              <TableHead
                key={column.key}
                className={cn(column.align === 'right' && 'text-right', column.className)}
              >
                {column.sortable && onSortChange ? (
                  <button
                    type="button"
                    onClick={() => toggleSort(column.key)}
                    className={cn(
                      'inline-flex items-center gap-1 uppercase tracking-wide transition-colors hover:text-slate-900',
                      column.align === 'right' && 'flex-row-reverse'
                    )}
                    aria-label={`Sort by ${column.header}`}
                  >
                    {column.header}
                    {activeField === column.key ? (
                      isDescending ? (
                        <ArrowDown className="h-3 w-3" />
                      ) : (
                        <ArrowUp className="h-3 w-3" />
                      )
                    ) : (
                      <ChevronsUpDown className="h-3 w-3 opacity-40" />
                    )}
                  </button>
                ) : (
                  column.header
                )}
              </TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((row) => (
            <TableRow
              key={rowKey(row)}
              onClick={onRowClick ? () => onRowClick(row) : undefined}
              className={cn(onRowClick && 'cursor-pointer')}
            >
              {columns.map((column) => (
                <TableCell
                  key={column.key}
                  className={cn(column.align === 'right' && 'text-right', column.className)}
                >
                  {column.render(row)}
                </TableCell>
              ))}
            </TableRow>
          ))}
        </TableBody>
      </Table>

      {pagination && onPageChange && pagination.total_pages > 1 && (
        <Pagination meta={pagination} onPageChange={onPageChange} />
      )}
    </div>
  );
}

export function Pagination({
  meta,
  onPageChange,
}: {
  meta: PaginationMeta;
  onPageChange: (page: number) => void;
}) {
  const from = (meta.page - 1) * meta.per_page + 1;
  const to = Math.min(meta.page * meta.per_page, meta.total);

  return (
    <div className="flex flex-wrap items-center justify-between gap-3">
      <p className="text-sm text-slate-500">
        Showing <span className="font-medium text-slate-700">{from}</span>–
        <span className="font-medium text-slate-700">{to}</span> of{' '}
        <span className="font-medium text-slate-700">{meta.total}</span>
      </p>
      <div className="flex items-center gap-2">
        <Button
          variant="outline"
          size="sm"
          disabled={meta.page <= 1}
          onClick={() => onPageChange(meta.page - 1)}
        >
          <ChevronLeft className="mr-1 h-4 w-4" />
          Previous
        </Button>
        <span className="text-sm text-slate-500">
          Page {meta.page} of {meta.total_pages}
        </span>
        <Button
          variant="outline"
          size="sm"
          disabled={meta.page >= meta.total_pages}
          onClick={() => onPageChange(meta.page + 1)}
        >
          Next
          <ChevronRight className="ml-1 h-4 w-4" />
        </Button>
      </div>
    </div>
  );
}
