import { ApiError } from './client';

export interface FieldKeyConflict {
  field: { uid: string; key: string; label: string; field_type: string; active: boolean; module_uid: string; module_name: string };
  suggested_key: string;
}

export function fieldKeyFailure(reason: unknown): FieldKeyConflict | null {
  if (!(reason instanceof ApiError) || !reason.payload || typeof reason.payload !== 'object') return null;
  const body = reason.payload as { error?: unknown; field_key_conflict?: FieldKeyConflict };
  const conflict = body.field_key_conflict;
  if (body.error !== 'field_key_conflict' || !conflict || !conflict.field) return null;
  const field = conflict.field;
  if (![field.uid, field.key, field.label, field.field_type, field.module_uid, field.module_name].every(v => typeof v === 'string')
    || typeof field.active !== 'boolean' || typeof conflict.suggested_key !== 'string'
    || !/^[a-z_][a-z0-9_]{0,63}$/.test(conflict.suggested_key)) return null;
  return conflict;
}

export function generatedFieldKey(label: string): string {
  return (label.trim().toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_+|_+$/g, '').replace(/^[0-9]+/, '') || 'field').slice(0, 64);
}
