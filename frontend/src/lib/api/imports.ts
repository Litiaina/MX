import { apiJson, apiUpload, jsonRequest } from './client';
import type { JsonValue } from './domain';
export interface ImportCell { text: string; kind: string }
export interface ImportSource { row: number; cells: Record<string, ImportCell> }
export interface ImportSheet { index: number; name: string; row_count: number; columns: number; sample: ImportSource[]; error?: string | null }
export interface ImportColumn { column: number; key: string; fill_down: boolean; date_format: string }
export interface ImportMapping { index: number; header_row: number; first_data_row: number; columns: ImportColumn[] }
export interface ImportValidation { ready: number; invalid: number; issues: { field: string; message: string; rows: number }[] }
export interface ImportJob { uid: string; filename: string; state: string; sheets: ImportSheet[]; config: { sheets?: ImportMapping[] }; created_at: number; total: number; processed: number; imported: number; failed: number; pending: number; validation?: ImportValidation }
export interface ImportRow extends ImportSource { sheet: number; values: Record<string, JsonValue>; preview_values?: Record<string, JsonValue>; errors: { field: string; column?: number; message: string; value?: string }[]; record_uid: string | null }
export interface ImportRows { rows: ImportRow[]; total: number; page: number; limit: number }
const path = (module: string, uid = '') => `/mx/v1/modules/${encodeURIComponent(module || 'mx-default-records')}/imports${uid ? `/${encodeURIComponent(uid)}` : ''}`;
export function uploadImport(module: string, file: File, uid: string, delimiter: string, onProgress: (loaded: number, total: number) => void) {
  const body = new FormData(); body.append('job_uid', uid); body.append('delimiter', delimiter); body.append('file', file);
  return apiUpload<ImportJob>(path(module), body, onProgress);
}
export const importHistory = (module: string, page = 1) => apiJson<{ jobs: ImportJob[]; total: number }>(`${path(module)}?page=${page}`);
export const importJob = (module: string, uid: string) => apiJson<ImportJob>(path(module, uid));
export const configureImport = (module: string, uid: string, sheets: ImportMapping[]) => apiJson<ImportJob>(`${path(module, uid)}/configure`, { ...jsonRequest('PUT', { sheets }), timeoutMs: 120_000, retrySafe: true, maxAttempts: 2 });
export const processImport = (module: string, uid: string) => apiJson<ImportJob>(`${path(module, uid)}/process`, { ...jsonRequest('POST', {}), timeoutMs: 120_000, retrySafe: true, maxAttempts: 2 });
export const importRows = (module: string, uid: string, state: string, page = 1, sheet?: number) => apiJson<ImportRows>(`${path(module, uid)}/rows?${new URLSearchParams({ state, page: String(page), ...(sheet === undefined ? {} : { sheet: String(sheet) }) })}`);
export const correctImport = (module: string, uid: string, row: ImportRow, values: Record<string, JsonValue>) => apiJson<{ record_uid: string }>(`${path(module, uid)}/retry`, { ...jsonRequest('POST', { sheet: row.sheet, row: row.row, values }), timeoutMs: 120_000, retrySafe: true, maxAttempts: 2 });
export const removeImportLog = (module: string, uid: string) => apiJson<{ response: string }>(path(module, uid), { method: 'DELETE' });
