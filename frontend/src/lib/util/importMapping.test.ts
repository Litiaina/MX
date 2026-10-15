import { describe, it, expect } from 'vitest';
import { guessMapping, csvCell, columnName, importWritable } from './importMapping';
import type { FieldDefinition } from '../api/domain';
import type { ImportSheet } from '../api/imports';
const fields = [{ key: 'name', label: 'Employee', field_type: 'text', active: true }, { key: 'date', label: 'Start date', field_type: 'date', active: true }, { key: 'total', label: 'Total', field_type: 'formula', active: true }] as FieldDefinition[];
const sheet: ImportSheet = { name: 'Sheet', index: 0, columns: 3, row_count: 2, sample: [{ row: 1, cells: { '1': { text: 'Notes', kind: 'text' } } }, { row: 2, cells: { '1': { text: 'Employee', kind: 'text' }, '2': { text: 'START DATE', kind: 'text' }, '3': { text: 'Employee', kind: 'text' } } }] };
describe('import mapping', () => {
  it('suggests exact, unique writable matches and finds the header', () => { const m = guessMapping(sheet, fields); expect(m.header_row).toBe(2); expect(m.first_data_row).toBe(3); expect(m.columns.map(c => c.key)).toEqual(['name','date','']); expect(m.columns.every(c => !c.fill_down)).toBe(true); });
  it('uses stable field tags and skips the human label row', () => { const m = guessMapping({ ...sheet, sample: [{ row: 1, cells: { '1': { text: 'mx:name', kind: 'text' }, '2': { text: 'mx:total', kind: 'text' } } }] }, fields); expect(m.first_data_row).toBe(3); expect(m.columns.map(c => c.key)).toEqual(['name','']); });
  it('does not guess when labels are ambiguous', () => { const m = guessMapping(sheet, [...fields, { ...fields[0], key: 'other' }]); expect(m.columns[0].key).toBe(''); });
  it('does not allow automatic fields or attachment paths', () => { expect(importWritable(fields[2])).toBe(false); expect(importWritable({ ...fields[0], field_type: 'attachments' })).toBe(false); });
  it('escapes log cells and neutralizes spreadsheet formulas', () => { expect(csvCell('=cmd()')).toBe('"\'=cmd()"'); expect(csvCell('A"B')).toBe('"A""B"'); expect(columnName(1)).toBe('A'); expect(columnName(27)).toBe('AA'); });
});
