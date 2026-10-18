import type { FieldDefinition } from '../api/domain';
import type { ImportMapping, ImportSheet } from '../api/imports';
export const importWritable = (field: FieldDefinition) => field.active && !['auto_number', 'attachments', 'formula', 'lookup', 'rollup'].includes(field.field_type);
export const importAttachmentFields = (fields: FieldDefinition[]) => fields.filter(field => field.active && field.field_type === 'attachments');
const normalize = (value: string) => value.trim().toLowerCase().replace(/[^a-z0-9]/g, '');
export function guessMapping(sheet: ImportSheet, fields: FieldDefinition[]): ImportMapping {
  const writable = fields.filter(importWritable);
  const score = (row: ImportSheet['sample'][number]) => Object.values(row.cells).filter(c => writable.some(f => c.text.trim() === `mx:${f.key}` || normalize(c.text) === normalize(f.label) || normalize(c.text) === normalize(f.key))).length;
  const header = sheet.sample.reduce((best, row) => score(row) > score(best) ? row : best, sheet.sample[0] || { row: 1, cells: {} });
  const tagged = Object.values(header.cells).some(c => c.text.trim().startsWith('mx:'));
  const used = new Set<string>();
  const columns = Object.entries(header.cells).map(([column, cell]) => {
    const matches = writable.filter(f => tagged ? cell.text.trim() === `mx:${f.key}` : normalize(cell.text) === normalize(f.label) || normalize(cell.text) === normalize(f.key));
    const key = matches.length === 1 && !used.has(matches[0].key) ? matches[0].key : '';
    if (key) used.add(key);
    return { column: Number(column), key, fill_down: false, date_format: 'iso' };
  });
  return { index: sheet.index, header_row: header.row, first_data_row: header.row + (tagged ? 2 : 1), columns };
}
export function restoreImportMapping(sheet: ImportSheet, saved: ImportMapping): ImportMapping {
  const columns = new Map(saved.columns.map(column => [column.column, { ...column }]));
  const header = sheet.sample.find(row => row.row === saved.header_row);
  for (const column of Object.keys(header?.cells || {}).map(Number)) {
    if (!columns.has(column)) columns.set(column, { column, key: '', fill_down: false, date_format: 'iso' });
  }
  return { ...saved, columns: [...columns.values()].sort((a, b) => a.column - b.column) };
}
export function columnName(column: number): string { let name = ''; for (let n = column; n; n = Math.floor((n - 1) / 26)) name = String.fromCharCode(65 + (n - 1) % 26) + name; return name; }
export function csvCell(value: string): string { const safe = /^[=+\-@\t\r]/.test(value) ? `'${value}` : value; return `"${safe.replaceAll('"', '""')}"`; }
