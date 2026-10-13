import type { ColumnSummary, FieldDefinition } from '../api/domain';
export const summaryNames = { sum: 'Total', average: 'Average', min: 'Minimum', max: 'Maximum', count: 'Count' };
export function canSummarize(field: FieldDefinition) { return ['integer','decimal','formula','auto_number'].includes(field.field_type); }
export function summaryValue(summary: ColumnSummary | undefined): string {
  if (!summary) return 'Unavailable';
  if (summary.value === null || !Number.isFinite(summary.value)) return '—';
  const decimals = summary.function === 'count' ? 0 : Math.min(12, Math.max(0, summary.decimals));
  return summary.value.toLocaleString(undefined, { minimumFractionDigits: decimals, maximumFractionDigits: decimals });
}
