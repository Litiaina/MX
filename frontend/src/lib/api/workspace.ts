import { apiFetch, apiJson, apiUpload, jsonRequest } from './client';
import type {
  ActionRateRow, AuditPage, BackupEntry, DashboardConfigResponse, DashboardWidget,
  DeploymentConfig, DeploymentResponse, FieldDefinition, JsonValue, MxRecord, PresenceResponse,
  RecordPage, SchemaResponse, StorageLayout, UserPerformanceRow, UserSummary
} from './domain';

export const loadDeployment = () => apiJson<DeploymentResponse>('/mx/v1/deployment/config', {}, false);
export const saveDeployment = (config: DeploymentConfig) => apiJson<{ response: string; revision: number; config: DeploymentConfig }>('/mx/v1/admin/deployment-config', jsonRequest('PUT', { config }));
export const loadSchema = (admin = false) => apiJson<SchemaResponse>(admin ? '/mx/v1/admin/schema' : '/mx/v1/schema');

export function listRecords(query: { page?: number; limit?: number; q?: string; match?: 'contains' | 'prefix' | 'exact'; filters?: Record<string, string>; attachments?: 'with' | 'without' | ''; sort_by?: string; sort_dir?: string } = {}) {
  const params = new URLSearchParams();
  if (query.page) params.set('page', String(query.page));
  if (query.limit) params.set('limit', String(query.limit));
  if (query.q) params.set('q', query.q);
  if (query.match) params.set('match', query.match);
  if (query.filters && Object.keys(query.filters).length) params.set('filters', JSON.stringify(query.filters));
  if (query.attachments) params.set('attachments', query.attachments);
  if (query.sort_by) params.set('sort_by', query.sort_by);
  if (query.sort_dir) params.set('sort_dir', query.sort_dir);
  return apiJson<RecordPage>(`/mx/v1/records?${params}`);
}
export const getRecord = (uid: string) => apiJson<MxRecord>(`/mx/v1/records/${encodeURIComponent(uid)}`);
export const createRecord = (values: Record<string, JsonValue>) => apiJson<MxRecord>('/mx/v1/records', jsonRequest('POST', { values }));
export const patchRecord = (uid: string, changes: Record<string, JsonValue>, base_revisions: Record<string, number>) =>
  apiJson<MxRecord>(`/mx/v1/records/${encodeURIComponent(uid)}`, jsonRequest('PATCH', { changes, base_revisions }));
export const deleteRecord = async (uid: string) => {
  const response = await apiFetch(`/mx/v1/records/${encodeURIComponent(uid)}`, { method: 'DELETE' });
  if (!response.ok) throw new Error((await response.json().catch(() => null))?.response || 'Could not delete record.');
};

export async function uploadAttachments(recordUid: string, fieldUid: string, files: File[], onProgress?: (loaded: number, total: number) => void) {
  const body = new FormData();
  for (const file of files) body.append('files', file);
  return apiUpload<{ attachments: unknown[] }>(`/mx/v1/records/${encodeURIComponent(recordUid)}/attachments/fields/${encodeURIComponent(fieldUid)}`, body, onProgress);
}
export const deleteAttachment = async (recordUid: string, attachmentUid: string) => {
  const response = await apiFetch(`/mx/v1/records/${encodeURIComponent(recordUid)}/attachments/${encodeURIComponent(attachmentUid)}`, { method: 'DELETE' });
  if (!response.ok) throw new Error((await response.json().catch(() => null))?.response || 'Could not delete attachment.');
};

export async function download(path: string, fallbackName: string) {
  const response = await apiFetch(path);
  if (!response.ok) throw new Error((await response.json().catch(() => null))?.response || 'Download failed.');
  const disposition = response.headers.get('content-disposition') || '';
  const fileName = /filename="?([^";]+)"?/i.exec(disposition)?.[1] || fallbackName;
  const url = URL.createObjectURL(await response.blob());
  const link = document.createElement('a');
  link.href = url; link.download = fileName; link.click();
  URL.revokeObjectURL(url);
}

export async function previewAttachment(recordUid: string, attachmentUid: string) {
  const response = await apiFetch(`/mx/v1/records/${encodeURIComponent(recordUid)}/attachments/${encodeURIComponent(attachmentUid)}/preview`);
  if (!response.ok) throw new Error((await response.json().catch(() => null))?.response || 'Preview failed.');
  const disposition = response.headers.get('content-disposition') || '';
  const blob = await response.blob();
  return {
    blob,
    fileName: /filename="?([^";]+)"?/i.exec(disposition)?.[1] || '',
    mimeType: response.headers.get('content-type') || blob.type || 'application/octet-stream'
  };
}

export const loadDashboardConfig = () => apiJson<DashboardConfigResponse>('/mx/v1/dashboard/config');
export const saveDashboardConfig = (widgets: DashboardWidget[], showUserPerformance: boolean) =>
  apiJson<{ response: string; revision: number }>('/mx/v1/admin/dashboard-config', jsonRequest('PUT', { config: { widgets, show_user_performance: showUserPerformance } }));

export function actionRate(widget: DashboardWidget, dateFrom = '', dateTo = '') {
  const params = new URLSearchParams();
  const keys = ['group_field_uid', 'action_field_uid', 'action_mode', 'action_value', 'attachment_field_uid', 'date_field_uid', 'time_bucket'] as const;
  for (const key of keys) if (widget[key] != null) params.set(key, String(widget[key]));
  if (dateFrom) params.set('date_from', dateFrom);
  if (dateTo) params.set('date_to', dateTo);
  return apiJson<{ rows: ActionRateRow[] }>(`/mx/v1/reports/action-rate?${params}`);
}
export function userPerformance(dateFrom = '', dateTo = '') {
  const params = new URLSearchParams(); if (dateFrom) params.set('date_from', dateFrom); if (dateTo) params.set('date_to', dateTo);
  return apiJson<{ rows: UserPerformanceRow[] }>(`/mx/v1/reports/user-performance?${params}`);
}
export function exportReport(kind: string, widget?: DashboardWidget, dateFrom = '', dateTo = '') {
  const params = new URLSearchParams({ kind });
  if (widget) for (const key of ['group_field_uid', 'action_field_uid', 'action_mode', 'action_value', 'attachment_field_uid', 'date_field_uid', 'time_bucket'] as const) if (widget[key] != null) params.set(key, String(widget[key]));
  if (dateFrom) params.set('date_from', dateFrom); if (dateTo) params.set('date_to', dateTo);
  return download(`/mx/v1/reports/export.csv?${params}`, `mx-${kind}.csv`);
}
export const loadPresence = () => apiJson<PresenceResponse>('/mx/v1/presence');
export const loadUsers = async () => (await apiJson<{ users: UserSummary[] }>('/mx/v1/auth/get', jsonRequest('POST', { filter: 'all', value: '' }))).users;
export const createUser = (data: { name: string; email: string; password: string; access_level: number }) => apiJson('/mx/v1/user/create', jsonRequest('POST', data));
export const updateUserAccess = (uid: string, access_level: number) => apiJson('/mx/v1/auth/modify', jsonRequest('PATCH', { filter: 'uid', value: uid, new_email: null, new_password: null, new_name: null, access_level }));
export const removeUser = (uid: string) => apiJson('/mx/v1/auth/delete', jsonRequest('DELETE', { filter: 'uid', value: uid }));
export const resetUserPassword = (uid: string, data: object) => apiJson(`/mx/v1/admin/accounts/${encodeURIComponent(uid)}/password-reset`, jsonRequest('POST', data));
export const resetUserSecurity = (uid: string, data: object) => apiJson(`/mx/v1/admin/accounts/${encodeURIComponent(uid)}/security-reset`, jsonRequest('POST', data));

export const createField = (field: Omit<FieldDefinition, 'uid' | 'position' | 'active'>) => apiJson('/mx/v1/admin/schema/fields', jsonRequest('POST', field));
export const updateField = (uid: string, changes: Partial<FieldDefinition>) => apiJson(`/mx/v1/admin/schema/fields/${encodeURIComponent(uid)}`, jsonRequest('PUT', changes));
export const updateSystemField = (key: string, changes: object) => apiJson(`/mx/v1/admin/schema/system/${encodeURIComponent(key)}`, jsonRequest('PUT', changes));
export const updateSchemaOrder = (items: { kind: string; id: string }[]) => apiJson('/mx/v1/admin/schema/order', jsonRequest('PUT', { items }));
export const loadStorageLayout = () => apiJson<StorageLayout>('/mx/v1/admin/storage-layout');
export const saveStorageLayout = (folder_field_uids: string[], file_prefix_field_uid: string | null) => apiJson<StorageLayout>('/mx/v1/admin/storage-layout', jsonRequest('PUT', { folder_field_uids, file_prefix_field_uid }));

export const listBackups = () => apiJson<{ backups: BackupEntry[]; backup_running: boolean }>('/mx/v1/admin/backups');
export const createBackup = () => apiJson('/mx/v1/admin/backups', { method: 'POST' });
export const verifyBackup = (uid: string) => apiJson(`/mx/v1/admin/backups/${encodeURIComponent(uid)}/verify`, { method: 'POST' });
export const downloadBackup = (uid: string) => download(`/mx/v1/admin/backups/${encodeURIComponent(uid)}/download`, 'mx-backup.db');

export function loadAudit(page = 1, user = '', action = '', result = 'all') {
  const params = new URLSearchParams({ page: String(page), limit: '100', user, action, result });
  return apiJson<AuditPage>(`/mx/v1/admin/audit?${params}`);
}
