import type { ModuleReadingMap } from '../api/domain';

// A separate context is created per signed-in Workspace, never a global account cache.
export const moduleReadingContext = Symbol('mx.personal-module-reading');

export function fieldIsBold(preferences: ModuleReadingMap, moduleUid: string, fieldUid: string): boolean {
  return preferences[moduleUid || 'mx-default-records']?.bold_fields.includes(fieldUid) === true;
}

export function withBoldFields(preferences: ModuleReadingMap, moduleUid: string, fields: string[]): ModuleReadingMap {
  const result = { ...preferences };
  const bold_fields = [...new Set(fields)].sort();
  if (bold_fields.length) result[moduleUid || 'mx-default-records'] = { bold_fields };
  else delete result[moduleUid || 'mx-default-records'];
  return result;
}
