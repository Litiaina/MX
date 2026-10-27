import type { ColumnPresentation, ColumnTone, ModuleReadingMap } from '../api/domain';

// A separate context is created per signed-in Workspace, never a global account cache.
export const moduleReadingContext = Symbol('mx.personal-module-reading');

export function fieldIsBold(preferences: ModuleReadingMap, moduleUid: string, fieldUid: string): boolean {
  return preferences[moduleUid || 'mx-default-records']?.bold_fields.includes(fieldUid) === true;
}

export function withBoldFields(preferences: ModuleReadingMap, moduleUid: string, fields: string[]): ModuleReadingMap {
  const result = { ...preferences };
  const bold_fields = [...new Set(fields)].sort();
  const columns = result[moduleUid || 'mx-default-records']?.columns;
  if (bold_fields.length || Object.keys(columns || {}).length) result[moduleUid || 'mx-default-records'] = { bold_fields, ...(columns && { columns }) };
  else delete result[moduleUid || 'mx-default-records'];
  return result;
}

export const columnTones: ColumnTone[] = ['auto', 'slate', 'blue', 'green', 'amber', 'violet', 'rose'];
export function columnPresentation(preferences: ModuleReadingMap, moduleUid: string, field: {uid: string; field_type: string}): ColumnPresentation {
  return preferences[moduleUid || 'mx-default-records']?.columns?.[field.uid]
    || { style: ['select', 'boolean'].includes(field.field_type) ? 'badge' : 'text', tone: 'auto' };
}
// Stable category colors, not guessed business meanings. No raw HTML or arbitrary CSS.
export function columnTone(presentation: ColumnPresentation, value: unknown, fieldType: string): ColumnTone | 'none' {
  if (presentation.tone !== 'auto') return columnTones.includes(presentation.tone) ? presentation.tone : 'slate';
  if (presentation.style !== 'badge') return 'none';
  if (fieldType === 'boolean') return value === true ? 'green' : 'slate';
  let hash = 0;
  for (const character of String(value ?? '')) hash = (Math.imul(hash, 31) + character.codePointAt(0)!) >>> 0;
  return columnTones[1 + hash % (columnTones.length - 1)];
}

export function withColumnPresentation(preferences: ModuleReadingMap, moduleUid: string, columns: Record<string, ColumnPresentation>): ModuleReadingMap {
  const key = moduleUid || 'mx-default-records';
  const result = { ...preferences };
  const current = { bold_fields: result[key]?.bold_fields || [] };
  if (Object.keys(columns).length) result[key] = { ...current, columns: { ...columns } };
  else if (current.bold_fields.length) result[key] = current;
  else delete result[key];
  return result;
}
