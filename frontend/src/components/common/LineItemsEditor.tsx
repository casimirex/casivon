import { Plus, Trash2 } from 'lucide-react';
import { useFieldArray, type UseFormReturn } from 'react-hook-form';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { formatMoney } from '@/lib/utils';
import { previewLine, sumLines, type LineFormValues } from '@/lib/lineMath';
import type { SelectOption } from '@/components/ui/Select';

export interface LineItemsEditorProps {
  form: UseFormReturn<LineFormValues>;
  /** Products to attach a line to; leave empty for free-text lines only. */
  productOptions?: SelectOption[];
  /** Purchase orders have no per-line discount. */
  showDiscount?: boolean;
  currency?: string;
  disabled?: boolean;
}

export function LineItemsEditor({
  form,
  productOptions = [],
  showDiscount = true,
  currency = 'USD',
  disabled = false,
}: LineItemsEditorProps) {
  const { control, register, watch, formState } = form;
  const { errors } = formState;
  const { fields, append, remove } = useFieldArray({ control, name: 'lines' });
  const lines = watch('lines') ?? [];
  const totals = sumLines(lines);

  // Zod reports "add at least one line" against the array itself.
  const arrayError = (errors.lines as { message?: string } | undefined)?.message;
  const lineErrors = (errors.lines ?? []) as Array<Record<string, { message?: string }>>;

  const addLine = () =>
    append({
      product_id: '',
      description: '',
      quantity: 1,
      unit_price: 0,
      discount_percent: 0,
      tax_rate: 0,
    });

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-slate-900">Line items</h3>
        <Button type="button" variant="outline" size="sm" onClick={addLine} disabled={disabled}>
          <Plus className="mr-1 h-4 w-4" />
          Add line
        </Button>
      </div>

      {arrayError && (
        <p className="text-xs font-medium text-red-600" role="alert">
          {arrayError}
        </p>
      )}

      <div className="overflow-x-auto rounded-md border">
        <table className="w-full text-sm">
          <thead className="border-b bg-slate-50">
            <tr>
              {productOptions.length > 0 && (
                <th className="px-3 py-2 text-left text-xs font-medium uppercase text-slate-500">
                  Product
                </th>
              )}
              <th className="px-3 py-2 text-left text-xs font-medium uppercase text-slate-500">
                Description
              </th>
              <th className="w-24 px-3 py-2 text-right text-xs font-medium uppercase text-slate-500">
                Qty
              </th>
              <th className="w-32 px-3 py-2 text-right text-xs font-medium uppercase text-slate-500">
                Unit price
              </th>
              {showDiscount && (
                <th className="w-24 px-3 py-2 text-right text-xs font-medium uppercase text-slate-500">
                  Disc %
                </th>
              )}
              <th className="w-24 px-3 py-2 text-right text-xs font-medium uppercase text-slate-500">
                Tax %
              </th>
              <th className="w-32 px-3 py-2 text-right text-xs font-medium uppercase text-slate-500">
                Line total
              </th>
              <th className="w-12 px-3 py-2" />
            </tr>
          </thead>
          <tbody>
            {fields.length === 0 && (
              <tr>
                <td
                  colSpan={productOptions.length > 0 ? 8 : 7}
                  className="px-3 py-8 text-center text-sm text-slate-400"
                >
                  No lines yet — add one to price this document.
                </td>
              </tr>
            )}

            {fields.map((field, index) => {
              const line = lines[index] ?? {};
              const { net } = previewLine(
                Number(line.quantity ?? 0),
                Number(line.unit_price ?? 0),
                Number(line.discount_percent ?? 0),
                Number(line.tax_rate ?? 0)
              );
              const rowError = lineErrors[index];

              return (
                <tr key={field.id} className="border-b last:border-0 align-top">
                  {productOptions.length > 0 && (
                    <td className="px-3 py-2">
                      <Select
                        options={productOptions}
                        placeholder="Free text"
                        disabled={disabled}
                        className="min-w-[12rem]"
                        {...register(`lines.${index}.product_id` as const)}
                      />
                    </td>
                  )}
                  <td className="px-3 py-2">
                    <Input
                      placeholder="What is being sold"
                      disabled={disabled}
                      className="min-w-[12rem]"
                      {...register(`lines.${index}.description` as const)}
                    />
                    {rowError?.description?.message && (
                      <p className="mt-1 text-xs text-red-600">{rowError.description.message}</p>
                    )}
                  </td>
                  <td className="px-3 py-2">
                    <Input
                      type="number"
                      min={1}
                      step={1}
                      className="text-right"
                      disabled={disabled}
                      {...register(`lines.${index}.quantity` as const)}
                    />
                    {rowError?.quantity?.message && (
                      <p className="mt-1 text-xs text-red-600">{rowError.quantity.message}</p>
                    )}
                  </td>
                  <td className="px-3 py-2">
                    <Input
                      type="number"
                      min={0}
                      step="0.01"
                      className="text-right"
                      disabled={disabled}
                      {...register(`lines.${index}.unit_price` as const)}
                    />
                    {rowError?.unit_price?.message && (
                      <p className="mt-1 text-xs text-red-600">{rowError.unit_price.message}</p>
                    )}
                  </td>
                  {showDiscount && (
                    <td className="px-3 py-2">
                      <Input
                        type="number"
                        min={0}
                        max={100}
                        step="0.01"
                        className="text-right"
                        disabled={disabled}
                        {...register(`lines.${index}.discount_percent` as const)}
                      />
                    </td>
                  )}
                  <td className="px-3 py-2">
                    <Input
                      type="number"
                      min={0}
                      step="0.01"
                      className="text-right"
                      disabled={disabled}
                      {...register(`lines.${index}.tax_rate` as const)}
                    />
                  </td>
                  <td className="px-3 py-3 text-right font-medium tabular-nums">
                    {formatMoney(net, currency)}
                  </td>
                  <td className="px-3 py-2 text-right">
                    <button
                      type="button"
                      onClick={() => remove(index)}
                      disabled={disabled}
                      className="rounded p-1.5 text-slate-400 transition-colors hover:bg-red-50 hover:text-red-600 disabled:opacity-40"
                      aria-label={`Remove line ${index + 1}`}
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      <div className="flex justify-end">
        <dl className="w-full max-w-xs space-y-1 text-sm">
          <div className="flex justify-between">
            <dt className="text-slate-500">Subtotal</dt>
            <dd className="tabular-nums">{formatMoney(totals.subtotal, currency)}</dd>
          </div>
          <div className="flex justify-between">
            <dt className="text-slate-500">Tax</dt>
            <dd className="tabular-nums">{formatMoney(totals.tax, currency)}</dd>
          </div>
          <div className="flex justify-between border-t pt-1 text-base font-semibold">
            <dt>Total</dt>
            <dd className="tabular-nums">{formatMoney(totals.total, currency)}</dd>
          </div>
        </dl>
      </div>
    </div>
  );
}
