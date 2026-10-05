import { apiFetch, apiJson, apiUpload, jsonRequest } from './client';
import type {
  ActionRateRow, AuditPage, BackupEntry, DashboardConfigResponse, DashboardWidget,
  DeploymentConfig, DeploymentResponse, FieldDefinition, JsonValue, MxRecord, PresenceResponse,
  PreferencesResponse, RecordPage, SchemaResponse, StorageLayout, UserPerformanceRow, UserPreferences, UserSummary,
  ActiveCallSummary, CallMode, CallParticipant, CallSignalKind, CallState, ChannelFile, ChannelMember, ChannelReadState, ChatMessage, CollaborationChannel, CollaborationPerson, MxNotification, ModuleDefinition, NotificationSoundInfo, RecordVersionDetail,
  RecordVersionSummary, TrashRecord, TrashRecordDetail, GlobalSearchResult,
  AccountModuleAccessResponse, AccountModuleGrant
} from './domain';

export const loadDeployment = () => apiJson<DeploymentResponse>('/mx/v1/deployment/config', {}, false);
export const saveDeployment = (config: DeploymentConfig) => apiJson<{ response: string; revision: number; config: DeploymentConfig }>('/mx/v1/admin/deployment-config', jsonRequest('PUT', { config }));
export const uploadDeploymentLogo = (file: File, onProgress?: (loaded: number, total: number) => void) => { const body = new FormData(); body.append('logo', file); return apiUpload<{ response: string; logo_url: string; config: DeploymentConfig }>('/mx/v1/admin/deployment-logo', body, onProgress); };
export const loadSchema = (admin = false) => apiJson<SchemaResponse>(admin ? '/mx/v1/admin/schema' : '/mx/v1/schema');
export const loadModules = () => apiJson<{ modules: ModuleDefinition[] }>('/mx/v1/modules');
export const loadModuleSchema = (moduleUid: string) => apiJson<SchemaResponse>(`/mx/v1/modules/${encodeURIComponent(moduleUid)}/schema`);
export const createModule = (data: { name: string; singular_name?: string; description?: string; icon?: string; color?: string; config?: Record<string, JsonValue> }) =>
  apiJson<{ module: ModuleDefinition }>('/mx/v1/admin/modules', jsonRequest('POST', data));
export const updateModule = (uid: string, changes: Partial<ModuleDefinition>) =>
  apiJson<{ module: ModuleDefinition }>(`/mx/v1/admin/modules/${encodeURIComponent(uid)}`, jsonRequest('PUT', changes));
export const deleteModule = (uid: string, credentials: { confirmation: string; admin_password: string; admin_otp: string | null; admin_recovery_code: string | null }) =>
  apiJson<{ response: string; module_uid: string; module_name: string; records_deleted: number; attachments_moved_to_n1_trash: number }>(`/mx/v1/admin/modules/${encodeURIComponent(uid)}`, jsonRequest('DELETE', credentials));
export const loadAccountModuleAccess = (uid: string) =>
  apiJson<AccountModuleAccessResponse>(`/mx/v1/admin/accounts/${encodeURIComponent(uid)}/modules`);
export const saveAccountModuleAccess = (uid: string, revision: number, grants: AccountModuleGrant[]) =>
  apiJson<{ response: string; revision: number }>(`/mx/v1/admin/accounts/${encodeURIComponent(uid)}/modules`, jsonRequest('PUT', { revision, grants }));

export function listRecords(query: { page?: number; limit?: number; q?: string; match?: 'contains' | 'prefix' | 'exact'; filters?: Record<string, string>; attachments?: 'with' | 'without' | ''; sort_by?: string; sort_dir?: string } = {}, moduleUid?: string) {
  const params = new URLSearchParams();
  if (query.page) params.set('page', String(query.page));
  if (query.limit) params.set('limit', String(query.limit));
  if (query.q) params.set('q', query.q);
  if (query.match) params.set('match', query.match);
  if (query.filters && Object.keys(query.filters).length) params.set('filters', JSON.stringify(query.filters));
  if (query.attachments) params.set('attachments', query.attachments);
  if (query.sort_by) params.set('sort_by', query.sort_by);
  if (query.sort_dir) params.set('sort_dir', query.sort_dir);
  const base = moduleUid ? `/mx/v1/modules/${encodeURIComponent(moduleUid)}/records` : '/mx/v1/records';
  return apiJson<RecordPage>(`${base}?${params}`);
}
export const getRecord = (uid: string, moduleUid?: string) => apiJson<MxRecord>(moduleUid ? `/mx/v1/modules/${encodeURIComponent(moduleUid)}/records/${encodeURIComponent(uid)}` : `/mx/v1/records/${encodeURIComponent(uid)}`);
export const createRecord = (values: Record<string, JsonValue>, moduleUid?: string) => apiJson<MxRecord>(moduleUid ? `/mx/v1/modules/${encodeURIComponent(moduleUid)}/records` : '/mx/v1/records', jsonRequest('POST', { values }));
export const patchRecord = (uid: string, changes: Record<string, JsonValue>, base_revision: number, moduleUid?: string) =>
  apiJson<MxRecord>(moduleUid ? `/mx/v1/modules/${encodeURIComponent(moduleUid)}/records/${encodeURIComponent(uid)}` : `/mx/v1/records/${encodeURIComponent(uid)}`, jsonRequest('PATCH', { changes, base_revision }));
export const deleteRecord = async (uid: string, moduleUid?: string) => {
  const path = moduleUid ? `/mx/v1/modules/${encodeURIComponent(moduleUid)}/records/${encodeURIComponent(uid)}` : `/mx/v1/records/${encodeURIComponent(uid)}`;
  const response = await apiFetch(path, { method: 'DELETE' });
  if (!response.ok) throw new Error((await response.json().catch(() => null))?.response || 'Could not delete record.');
};
function recordPath(uid: string, moduleUid?: string) { return moduleUid ? `/mx/v1/modules/${encodeURIComponent(moduleUid)}/records/${encodeURIComponent(uid)}` : `/mx/v1/records/${encodeURIComponent(uid)}`; }
export const listRecordVersions = (uid: string, moduleUid?: string) => apiJson<{ versions: RecordVersionSummary[] }>(`${recordPath(uid,moduleUid)}/versions`);
export const getRecordVersion = (uid: string, versionUid: string, moduleUid?: string) => apiJson<RecordVersionDetail>(`${recordPath(uid,moduleUid)}/versions/${encodeURIComponent(versionUid)}`);
export const restoreRecordVersion = (uid: string, versionUid: string, moduleUid?: string) => apiJson<{ response: string }>(`${recordPath(uid,moduleUid)}/versions/${encodeURIComponent(versionUid)}`, { method: 'POST' });
export const listTrash = () => apiJson<{ records: TrashRecord[] }>('/mx/v1/admin/trash');
export const getTrashRecord = (uid: string) => apiJson<{ record: TrashRecordDetail }>(`/mx/v1/admin/trash/${encodeURIComponent(uid)}`);
export const restoreTrashRecord = (uid: string) => apiJson<{ response: string; uid: string }>(`/mx/v1/admin/trash/${encodeURIComponent(uid)}/restore`, { method: 'POST' });

export async function uploadAttachments(recordUid: string, fieldUid: string, files: File[], baseRevision: number, onProgress?: (loaded: number, total: number) => void) {
  const body = new FormData();
  body.append('base_revision', String(baseRevision));
  for (const file of files) body.append('files', file);
  return apiUpload<{ attachments: unknown[]; record_revision: number }>(`/mx/v1/records/${encodeURIComponent(recordUid)}/attachments/fields/${encodeURIComponent(fieldUid)}`, body, onProgress);
}
export const deleteAttachment = async (recordUid: string, attachmentUid: string, baseRevision: number) => {
  const response = await apiFetch(`/mx/v1/records/${encodeURIComponent(recordUid)}/attachments/${encodeURIComponent(attachmentUid)}?base_revision=${encodeURIComponent(baseRevision)}`, { method: 'DELETE' });
  if (!response.ok) throw new Error((await response.json().catch(() => null))?.response || 'Could not delete attachment.');
  return response.json() as Promise<{ response: string; record_revision: number }>;
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

export async function previewAttachment(recordUid: string, attachmentUid: string, fileName = '', mimeType = '') {
  const path = `/mx/v1/records/${encodeURIComponent(recordUid)}/attachments/${encodeURIComponent(attachmentUid)}/preview`;
  if (!/\.(docx?|xlsx?|pptx?|odt|ods|odp)$/i.test(fileName)) {
    const ticket = await issueRecordAttachmentPreviewTicket(recordUid, attachmentUid);
    return { url: ticket.url, fileName: ticket.file_name || fileName, mimeType: ticket.mime_type || mimeType || 'application/octet-stream' };
  }
  const response = await apiFetch(path);
  if (!response.ok) throw new Error((await response.json().catch(() => null))?.response || 'Preview failed.');
  const disposition = response.headers.get('content-disposition') || '';
  const blob = await response.blob();
  return {
    blob,
    url: undefined,
    fileName: /filename="?([^";]+)"?/i.exec(disposition)?.[1] || '',
    mimeType: response.headers.get('content-type') || blob.type || 'application/octet-stream'
  };
}

export const issueRecordAttachmentPreviewTicket = (recordUid: string, attachmentUid: string) =>
  apiJson<{ url: string; file_name: string; mime_type: string; expires_in_seconds: number }>(
    `/mx/v1/records/${encodeURIComponent(recordUid)}/attachments/${encodeURIComponent(attachmentUid)}/preview-ticket`,
    { method: 'POST' }
  );

export const loadDashboardConfig = () => apiJson<DashboardConfigResponse>('/mx/v1/dashboard/config');
export const saveDashboardConfig = (widgets: DashboardWidget[], showUserPerformance: boolean) =>
  apiJson<{ response: string; revision: number }>('/mx/v1/admin/dashboard-config', jsonRequest('PUT', { config: { widgets, show_user_performance: showUserPerformance } }));

export function actionRate(widget: DashboardWidget, dateFrom = '', dateTo = '') {
  const params = new URLSearchParams();
  const keys = ['module_uid', 'group_field_uid', 'secondary_group_field_uid', 'action_field_uid', 'action_mode', 'action_value', 'attachment_field_uid', 'date_field_uid', 'time_bucket'] as const;
  for (const key of keys) if (widget[key] != null) params.set(key, String(widget[key]));
  if (widget.filters?.length) params.set('filters', JSON.stringify(widget.filters));
  if (dateFrom) params.set('date_from', dateFrom);
  if (dateTo) params.set('date_to', dateTo);
  return apiJson<{ rows: ActionRateRow[] }>(`/mx/v1/reports/action-rate?${params}`);
}
export function userPerformance(dateFrom = '', dateTo = '') {
  const params = new URLSearchParams(); if (dateFrom) params.set('date_from', dateFrom); if (dateTo) params.set('date_to', dateTo);
  return apiJson<{ rows: UserPerformanceRow[] }>(`/mx/v1/reports/user-performance?${params}`);
}
export function exportReport(kind: string, widget?: DashboardWidget, dateFrom = '', dateTo = '', moduleUid = '') {
  const params = new URLSearchParams({ kind });
  if (widget) { for (const key of ['module_uid', 'group_field_uid', 'secondary_group_field_uid', 'action_field_uid', 'action_mode', 'action_value', 'attachment_field_uid', 'date_field_uid', 'time_bucket'] as const) if (widget[key] != null) params.set(key, String(widget[key])); if (widget.filters?.length) params.set('filters', JSON.stringify(widget.filters)); }
  else if (moduleUid) params.set('module_uid', moduleUid);
  if (dateFrom) params.set('date_from', dateFrom); if (dateTo) params.set('date_to', dateTo);
  return download(`/mx/v1/reports/export.csv?${params}`, `mx-${kind}.csv`);
}
export const loadPresence = () => apiJson<PresenceResponse>('/mx/v1/presence');
export const loadPreferences = () => apiJson<PreferencesResponse>('/mx/v1/account/preferences');
export const savePreferences = (preferences: UserPreferences) =>
  apiJson<PreferencesResponse & { response: string }>('/mx/v1/account/preferences', jsonRequest('PUT', { preferences }));
export const loadNotificationSoundInfo = () => apiJson<NotificationSoundInfo>('/mx/v1/account/notification-sound/info');
export const uploadNotificationSound = (file: File, onProgress?: (loaded: number, total: number) => void) => {
  const body = new FormData(); body.append('sound', file);
  return apiUpload<NotificationSoundInfo>('/mx/v1/account/notification-sound', body, onProgress);
};
export const deleteNotificationSound = () => apiJson<{ response: string }>('/mx/v1/account/notification-sound', { method: 'DELETE' });
export async function loadNotificationSound(version?: number) {
  const suffix = version ? `?v=${encodeURIComponent(String(version))}` : '';
  const response = await apiFetch(`/mx/v1/account/notification-sound${suffix}`);
  if (!response.ok) throw new Error((await response.json().catch(() => null))?.response || 'Could not load the notification sound.');
  return response.blob();
}
export const loadNotifications = (unreadOnly = false) =>
  apiJson<{ notifications: MxNotification[]; unread: number }>(`/mx/v1/notifications?unread_only=${unreadOnly}`);
export const markNotificationRead = (uid: string) =>
  apiJson<void>(`/mx/v1/notifications/${encodeURIComponent(uid)}/read`, { method: 'POST' });
export const markAllNotificationsRead = () =>
  apiJson<{ response: string; updated: number }>('/mx/v1/notifications/read-all', { method: 'POST' });
export const globalSearch = (q: string, limit = 30) =>
  apiJson<{ results: GlobalSearchResult[] }>(`/mx/v1/search?q=${encodeURIComponent(q)}&limit=${limit}`);
export const listChannels = () => apiJson<{ channels: CollaborationChannel[]; file_max_size_bytes?: number }>('/mx/v1/collaboration/channels');
export const listCollaborationPeople = () => apiJson<{ people: CollaborationPerson[] }>('/mx/v1/collaboration/people');
export const createChannel = (data: { name: string; description: string; member_uids: string[]; kind: 'channel' | 'group' }) =>
  apiJson<{ uid: string }>('/mx/v1/collaboration/channels', jsonRequest('POST', data));
export const createDirectChannel = (user_uid: string) =>
  apiJson<{ uid: string; created: boolean }>('/mx/v1/collaboration/direct', jsonRequest('POST', { user_uid }));
export const updateChannel = (channelUid: string, name: string, description: string, invite_policy?: CollaborationChannel['invite_policy']) =>
  apiJson<{ response: string }>(`/mx/v1/collaboration/channels/${encodeURIComponent(channelUid)}`, jsonRequest('PUT', { name, description, invite_policy }));
export const listChannelMembers = (channelUid: string) =>
  apiJson<{ members: ChannelMember[] }>(`/mx/v1/collaboration/channels/${encodeURIComponent(channelUid)}/members`);
export const saveChannelMember = (channelUid: string, user_uid: string, role: 'owner' | 'admin' | 'member' = 'member') =>
  apiJson<{ response: string; member: ChannelMember }>(`/mx/v1/collaboration/channels/${encodeURIComponent(channelUid)}/members`, jsonRequest('POST', { user_uid, role }));
export const removeChannelMember = async (channelUid: string, userUid: string) => {
  const response = await apiFetch(`/mx/v1/collaboration/channels/${encodeURIComponent(channelUid)}/members/${encodeURIComponent(userUid)}`, { method: 'DELETE' });
  if (!response.ok) throw new Error((await response.json().catch(() => null))?.response || 'Could not remove the space member.');
};
export const searchChannel = (channelUid: string, q: string, limit = 50) =>
  apiJson<{ messages: ChatMessage[] }>(`/mx/v1/collaboration/channels/${encodeURIComponent(channelUid)}/search?q=${encodeURIComponent(q)}&limit=${limit}`);
export const listChannelFiles = (channelUid: string, q = '', limit = 50, offset = 0) =>
  apiJson<{ files: ChannelFile[]; total: number }>(`/mx/v1/collaboration/channels/${encodeURIComponent(channelUid)}/files?q=${encodeURIComponent(q)}&limit=${limit}&offset=${offset}`);
export const listMessages = (channelUid: string, before?: number) => {
  const params = new URLSearchParams(); if (before) params.set('before', String(before));
  const query = params.size ? `?${params}` : '';
  return apiJson<{ messages: ChatMessage[]; pinned_messages: ChatMessage[]; read_states: ChannelReadState[]; has_more: boolean; page_size: number }>(`/mx/v1/collaboration/channels/${encodeURIComponent(channelUid)}/messages${query}`);
};
export const sendMessage = (channelUid: string, body: string, mention_uids: string[] = [], record_uids: string[] = [], reply_to_uid: string | null = null) =>
  apiJson<{ message: ChatMessage }>(`/mx/v1/collaboration/channels/${encodeURIComponent(channelUid)}/messages`, jsonRequest('POST', { body, mention_uids, record_uids, reply_to_uid }));
export const editMessage = (messageUid: string, body: string) =>
  apiJson<{ message: ChatMessage }>(`/mx/v1/collaboration/messages/${encodeURIComponent(messageUid)}`, jsonRequest('PUT', { body }));
export const deleteMessage = async (messageUid: string) => {
  const response = await apiFetch(`/mx/v1/collaboration/messages/${encodeURIComponent(messageUid)}`, { method: 'DELETE' });
  if (!response.ok) throw new Error((await response.json().catch(() => null))?.response || 'Could not delete the message.');
};
export const toggleMessageReaction = (messageUid: string, emoji: string) =>
  apiJson<{ message: ChatMessage }>(`/mx/v1/collaboration/messages/${encodeURIComponent(messageUid)}/reactions`, jsonRequest('POST', { emoji }));
export const setMessagePinned = (messageUid: string, pinned: boolean) =>
  apiJson<{ message: ChatMessage }>(`/mx/v1/collaboration/messages/${encodeURIComponent(messageUid)}/pin`, { method: pinned ? 'PUT' : 'DELETE' });
export const markChannelRead = (channelUid: string) =>
  apiJson<{ read_state: ChannelReadState }>(`/mx/v1/collaboration/channels/${encodeURIComponent(channelUid)}/read`, { method: 'POST' });
export const getCallState = (channelUid: string) =>
  apiJson<CallState>(`/mx/v1/collaboration/channels/${encodeURIComponent(channelUid)}/call`);
export const listActiveCalls = () => apiJson<{ calls: ActiveCallSummary[] }>('/mx/v1/collaboration/calls');
export const joinCall = (channelUid: string, mode: CallMode, session_uid: string) =>
  apiJson<CallState>(`/mx/v1/collaboration/channels/${encodeURIComponent(channelUid)}/call`, jsonRequest('POST', { mode, session_uid }));
export const updateCallParticipant = (channelUid: string, session_uid: string, update: Partial<Pick<CallParticipant, 'audio_enabled' | 'video_enabled' | 'screen_sharing'>>) =>
  apiJson<CallState>(`/mx/v1/collaboration/channels/${encodeURIComponent(channelUid)}/call`, jsonRequest('PATCH', { ...update, session_uid }));
export const leaveCall = (channelUid: string, sessionUid: string) =>
  apiJson<void>(`/mx/v1/collaboration/channels/${encodeURIComponent(channelUid)}/call?session_uid=${encodeURIComponent(sessionUid)}`, { method: 'DELETE', keepalive: true });
export const sendCallSignal = (channelUid: string, recipient_uid: string, sender_session_uid: string, recipient_session_uid: string, kind: CallSignalKind, data: object) =>
  apiJson<void>(`/mx/v1/collaboration/channels/${encodeURIComponent(channelUid)}/call/signal`, jsonRequest('POST', { recipient_uid, sender_session_uid, recipient_session_uid, kind, data }));
export const uploadMessageFile = (messageUid: string, file: File, onProgress?: (loaded: number, total: number) => void) => {
  const body = new FormData(); body.append('file', file);
  return apiUpload<{ file: unknown }>(`/mx/v1/collaboration/messages/${encodeURIComponent(messageUid)}/files`, body, onProgress);
};
export async function previewMessageFile(fileUid: string) {
  const response = await apiFetch(`/mx/v1/collaboration/files/${encodeURIComponent(fileUid)}/preview`);
  if (!response.ok) throw new Error((await response.json().catch(() => null))?.response || 'Preview failed.');
  const disposition = response.headers.get('content-disposition') || '';
  const blob = await response.blob();
  return {
    blob,
    fileName: /filename="?([^";]+)"?/i.exec(disposition)?.[1] || '',
    mimeType: response.headers.get('content-type') || blob.type || 'application/octet-stream'
  };
}
export const issueMessageFilePreviewTicket = (fileUid: string) =>
  apiJson<{ url: string; file_name: string; mime_type: string; expires_in_seconds: number }>(
    `/mx/v1/collaboration/files/${encodeURIComponent(fileUid)}/preview-ticket`,
    { method: 'POST' }
  );

export async function prepareMessageFilePreview(fileUid: string, fileName: string) {
  if (/\.(docx?|xlsx?|pptx?|odt|ods|odp)$/i.test(fileName)) {
    return previewMessageFile(fileUid);
  }
  const ticket = await issueMessageFilePreviewTicket(fileUid);
  return {
    url: ticket.url,
    fileName: ticket.file_name || fileName,
    mimeType: ticket.mime_type || 'application/octet-stream'
  };
}
export const loadUsers = async () => (await apiJson<{ users: UserSummary[] }>('/mx/v1/auth/get', jsonRequest('POST', { filter: 'all', value: '' }))).users;
export const createUser = (data: { name: string; email: string; password: string; access_level: number }) => apiJson('/mx/v1/user/create', jsonRequest('POST', data));
export const updateUserAccess = (uid: string, access_level: number) => apiJson('/mx/v1/auth/modify', jsonRequest('PATCH', { filter: 'uid', value: uid, new_email: null, new_password: null, new_name: null, access_level }));
export const removeUser = (uid: string) => apiJson('/mx/v1/auth/delete', jsonRequest('DELETE', { filter: 'uid', value: uid }));
export const resetUserPassword = (uid: string, data: object) => apiJson(`/mx/v1/admin/accounts/${encodeURIComponent(uid)}/password-reset`, jsonRequest('POST', data));
export const resetUserSecurity = (uid: string, data: object) => apiJson(`/mx/v1/admin/accounts/${encodeURIComponent(uid)}/security-reset`, jsonRequest('POST', data));

export const createField = (field: Omit<FieldDefinition, 'uid' | 'position' | 'active'>, moduleUid?: string) => apiJson(moduleUid ? `/mx/v1/admin/modules/${encodeURIComponent(moduleUid)}/fields` : '/mx/v1/admin/schema/fields', jsonRequest('POST', field));
export const updateField = (uid: string, changes: Partial<FieldDefinition>) => apiJson(`/mx/v1/admin/schema/fields/${encodeURIComponent(uid)}`, jsonRequest('PUT', changes));
export const updateSystemField = (key: string, changes: object) => apiJson(`/mx/v1/admin/schema/system/${encodeURIComponent(key)}`, jsonRequest('PUT', changes));
export const updateSchemaOrder = (items: { kind: string; id: string }[], moduleUid?: string) => apiJson(moduleUid && moduleUid !== 'mx-default-records' ? `/mx/v1/admin/modules/${encodeURIComponent(moduleUid)}/schema/order` : '/mx/v1/admin/schema/order', jsonRequest('PUT', { items }));
export const loadStorageLayout = (moduleUid?: string) => apiJson<StorageLayout>(moduleUid ? `/mx/v1/admin/modules/${encodeURIComponent(moduleUid)}/storage-layout` : '/mx/v1/admin/storage-layout');
export const saveStorageLayout = async (folder_field_uids: string[], file_prefix_field_uid: string | null, moduleUid?: string) =>
  (await apiJson<{ response: string; layout: StorageLayout }>(moduleUid ? `/mx/v1/admin/modules/${encodeURIComponent(moduleUid)}/storage-layout` : '/mx/v1/admin/storage-layout', jsonRequest('PUT', { folder_field_uids, file_prefix_field_uid }))).layout;

export const listBackups = () => apiJson<{ backups: BackupEntry[]; backup_running: boolean }>('/mx/v1/admin/backups');
export const createBackup = () => apiJson('/mx/v1/admin/backups', { method: 'POST' });
export const verifyBackup = (uid: string) => apiJson(`/mx/v1/admin/backups/${encodeURIComponent(uid)}/verify`, { method: 'POST' });
export const downloadBackup = (uid: string) => download(`/mx/v1/admin/backups/${encodeURIComponent(uid)}/download`, 'mx-backup.db');

export function loadAudit(page = 1, user = '', action = '', result = 'all') {
  const params = new URLSearchParams({ page: String(page), limit: '100', user, action, result });
  return apiJson<AuditPage>(`/mx/v1/admin/audit?${params}`);
}
