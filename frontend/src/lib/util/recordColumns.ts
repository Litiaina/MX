type Column = { key: string; field_type: string };
export interface RecordColumnSnapshot { keys: string[]; widths: number[]; viewport: number }
const keys = (fields: Column[]) => fields.map(field => `${field.key}:${field.field_type}`);

export function validColumnSnapshot(value: unknown, fields: Column[], viewport: number): value is RecordColumnSnapshot {
  if (!value || typeof value !== 'object') return false;
  const snapshot = value as RecordColumnSnapshot;
  return Array.isArray(snapshot.keys) && JSON.stringify(snapshot.keys) === JSON.stringify(keys(fields))
    && Array.isArray(snapshot.widths) && snapshot.widths.length === fields.length + 2
    && snapshot.widths.every(width => Number.isFinite(width) && width >= 24 && width <= 16384)
    && Number.isFinite(snapshot.viewport) && Math.abs(snapshot.viewport - viewport) <= 2;
}

export function snapshotRecordColumns(fields: Column[], viewport: number, widths: number[]): RecordColumnSnapshot {
  return {keys: keys(fields), viewport, widths};
}

/** Stable virtual columns: dates/numbers do not consume a name-sized column.
 * A measured paged layout takes precedence, so switching modes keeps its widths. */
export function recordColumnWidths(fields: Column[], viewport: number, snapshot?: unknown): number[] {
  if (validColumnSnapshot(snapshot, fields, viewport)) return [...snapshot.widths];
  const sizes: Record<string, number> = {date: 128, integer: 128, decimal: 144, formula: 144,
    boolean: 112, select: 160, long_text: 400, attachments: 160, rollup: 144};
  const widths = [52, ...fields.map(field => sizes[field.field_type] || 240), 134];
  const extra = Math.max(0, viewport - widths.reduce((sum, width) => sum + width, 0));
  let flexible = fields.flatMap((field, index) => field.field_type === 'long_text' ? [index + 1] : []);
  if (!flexible.length) flexible = fields.map((_, index) => index + 1);
  for (const index of flexible) widths[index] += extra / flexible.length;
  return widths;
}
