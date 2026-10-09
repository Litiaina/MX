import { apiJson, apiFile, apiUpload, apiUploadPart } from './client';
import { UploadMeter } from '../util/uploadProgress';

const root = '/mx/v1/drive';
export type DriveTab = 'mine' | 'shared' | 'recent' | 'starred' | 'trash';
export interface DriveItem {
  uid: string; owner_uid: string; parent_uid: string | null; name: string; kind: 'file' | 'folder'; revision: number;
  mime_type: string; size: number; created_at: number; updated_at: number; trashed_at: number | null;
  permission: 'owner' | 'editor' | 'viewer' | 'guest'; starred: boolean;
}
export type DriveSort = 'modified_desc' | 'modified_asc' | 'name_asc' | 'name_desc';
export interface DriveListOptions { limit?: number; sort?: DriveSort; kind?: '' | 'file' | 'folder' }
export interface DrivePage { items: DriveItem[]; total: number; offset: number; limit: number; page: number; total_pages: number; has_more: boolean; used_bytes: number; reserved_bytes: number; quota_bytes: number; quota_assigned: boolean; max_file_size_bytes: null; public_links: boolean; breadcrumbs: { uid: string; name: string }[] }
export interface DriveQuota { user_uid: string; quota_bytes: number; default_quota_bytes: number; used_bytes: number; reserved_bytes: number; assigned: boolean; revision: number }
export const driveQuota = (uid: string) => apiJson<DriveQuota>(`${root}/admin/quotas/${encodeURIComponent(uid)}`);
export const saveDriveQuota = (uid: string, base_revision: number, quota_bytes: number | null) => apiJson<DriveQuota>(`${root}/admin/quotas/${encodeURIComponent(uid)}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ base_revision, quota_bytes }) });
export interface DriveHistory { versions: { uid: string; file_name: string; mime_type: string; size: number; created_at: number }[]; activity: { action: string; actor: string; created_at: number }[] }
export interface DriveSpace { uid: string; name: string; kind: string; members: number }
export interface DriveSharing { users: { user_uid: string; name: string; email: string; role: 'viewer' | 'editor' }[]; spaces: { space_uid: string; name: string; kind: string; role: 'viewer' | 'editor'; members: number; archived: boolean }[]; inherited: {name: string; detail: string; role: string; kind: string; from: string}[]; links: { uid: string; expires_at: number | null; created_at: number }[] }
const listParameters = (view: DriveTab,parent: string | null,q: string,offset: number,options: DriveListOptions = {}) => new URLSearchParams({ view,q,offset:String(offset),limit:String(options.limit ?? 50),sort:options.sort ?? 'modified_desc',kind:options.kind ?? '',...(parent ? {parent_uid:parent} : {}) });
export const driveList = (view: DriveTab, parent: string | null, q = '', offset = 0, options: DriveListOptions = {}) => apiJson<DrivePage>(`${root}?${listParameters(view,parent,q,offset,options)}`);
export interface DriveNeighbors { previous: DriveItem | null; next: DriveItem | null; position: number; total: number }
export const driveNeighbors = (uid: string,view: DriveTab,parent: string | null,q = '',sort: DriveSort = 'modified_desc',signal?: AbortSignal) => apiJson<DriveNeighbors>(`${root}/items/${uid}/neighbors?${listParameters(view,parent,q,0,{sort})}`,{signal});
export const driveSpaces = (q: string) => apiJson<{spaces: DriveSpace[]}>(`${root}/spaces?${new URLSearchParams({q})}`);
export const driveSpaceGrant = (uid: string,space_uid: string,role: 'viewer' | 'editor' | null) => apiJson(`${root}/items/${uid}/sharing`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({space_uid,role}),retrySafe:true});
export const driveFolder = (name: string, parent_uid: string | null, operation_uid = crypto.randomUUID()) => apiJson<DriveItem>(`${root}/folders`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, parent_uid, operation_uid }), retrySafe: true });
export const driveChange = (item: DriveItem, action: 'rename' | 'move' | 'trash' | 'restore', value?: string | null) => apiJson<DriveItem>(`${root}/items/${item.uid}`, { method: 'PATCH', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ base_revision: item.revision, action, ...(action === 'rename' ? { name: value } : { parent_uid: value }) }) });
export const driveTransfer = (items: DriveItem[], action: 'copy' | 'move', parent_uid: string | null, operation_uid: string = crypto.randomUUID()) => apiJson<{items: DriveItem[]; action: string}>(`${root}/transfers`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({operation_uid, action, parent_uid, items: items.map(({uid, revision}) => ({uid, base_revision: revision}))}), retrySafe: true });
export const driveStar = (item: DriveItem) => apiJson(`${root}/items/${item.uid}/star`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ starred: !item.starred }), retrySafe: true });
export const drivePurge = (uid?: string) => apiJson(`${root}/${uid ? `items/${uid}` : 'trash/empty'}`, { method: uid ? 'DELETE' : 'POST', retrySafe: !uid });
export const driveSharing = (uid: string) => apiJson<DriveSharing>(`${root}/items/${uid}/sharing`);
export const driveGrant = (uid: string, user_uid: string, role: 'viewer' | 'editor' | null) => apiJson(`${root}/items/${uid}/sharing`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ user_uid, role }), retrySafe: true });
export const drivePeople = (q: string) => apiJson<{ people: { uid: string; name: string; email: string }[] }>(`${root}/people?${new URLSearchParams({ q })}`);
export const driveLink = (uid: string, expires_at: number | null) => apiJson<{ uid: string; token: string }>(`${root}/items/${uid}/links`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ expires_at }) });
export const driveRevokeLink = (uid: string, link: string) => apiJson(`${root}/items/${uid}/links/${link}`, { method: 'DELETE', retrySafe: true });
export const driveHistory = (uid: string) => apiJson<DriveHistory>(`${root}/items/${uid}/history`);
export const driveRestoreVersion = (item: DriveItem, version_uid: string, operation_uid: string = crypto.randomUUID()) => apiJson<DriveItem>(`${root}/items/${item.uid}/restore-version`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({base_revision:item.revision,version_uid,operation_uid}),retrySafe:true});
export async function drivePreview(item: DriveItem, version_uid?: string) {
  const ticket = await apiJson<{ url: string; mime_type: string; original_file_name: string }>(`${root}/items/${item.uid}/ticket`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ version_uid }) });
  const office = /\.(docx?|docm|rtf|xlsx?|xlsm|pptx?|pptm|odt|ods|odp)$/i.test(ticket.original_file_name);
  return { url: `${ticket.url}${office ? '?office=true' : ''}`, fileName: office ? `${item.name}.pdf` : item.name, mimeType: office ? 'application/pdf' : ticket.mime_type };
}
export async function driveFile(item: DriveItem, version_uid?: string): Promise<File> {
  const result = await apiFile(`${root}/items/${item.uid}/download${version_uid ? `?version_uid=${encodeURIComponent(version_uid)}` : ''}`);
  return new File([result.blob], item.name, { type: result.blob.type || item.mime_type });
}
export async function driveDownload(item: DriveItem, version_uid?: string) {
  // Use a short-lived ACL-checked ticket so native download can stream instead of buffering the entire file.
  const ticket = await apiJson<{ url: string }>(`${root}/items/${item.uid}/ticket`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ version_uid }) });
  const link = document.createElement('a'); link.href = `${ticket.url}?download=true`; link.download = item.name; link.rel = 'noreferrer'; link.click();
}

interface UploadState { operation_uid?: string; completed: boolean; item?: DriveItem; part_size?: number; workers?: number; uploaded_parts?: number[]; ready_to_finalize?: boolean }
export interface UploadProgress { loaded: number; total: number; phase: 'initializing' | 'uploading' | 'finalizing'; resumedParts: number; bytesPerSecond: number; etaSeconds: number | null }
export interface DriveUploadOptions { accountUid: string; parentUid: string | null; versionOf?: DriveItem; signal?: AbortSignal; onProgress?: (progress: UploadProgress) => void }
const pageOperations = new Map<string, { uid: string; time: number }>();
function pageOperation(key: string): string {
  const cached = pageOperations.get(key);
  if (cached && Date.now() - cached.time < 24 * 60 * 60 * 1000) return cached.uid;
  for (const [identity, entry] of pageOperations) if (Date.now() - entry.time >= 86400_000) pageOperations.delete(identity);
  const uid = crypto.randomUUID();
  pageOperations.set(key, { uid, time: Date.now() });
  return uid;
}
function uploadIdentity(file: File, options: DriveUploadOptions) {
  const { signal } = options;
  signal?.throwIfAborted();
  // Metadata only, as in console. N1 and MX own durable session recovery.
  const identity = JSON.stringify([options.accountUid, options.parentUid, options.versionOf?.uid, options.versionOf?.revision, file.name, file.size, file.type, file.lastModified]);
  return { key: identity, operation_uid: pageOperation(identity), last_modified: file.lastModified };
}
export async function uploadDriveFile(file: File, options: DriveUploadOptions): Promise<DriveItem> {
  const { signal, onProgress } = options;
  const progress = (phase: UploadProgress['phase'], loaded: number, resumedParts = 0) => onProgress?.({ phase, loaded, total: file.size, resumedParts, bytesPerSecond: 0, etaSeconds: null });
  progress('initializing', 0);
  const identity = uploadIdentity(file, options);
  const state = await apiJson<UploadState>(`${root}/uploads`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ operation_uid: identity.operation_uid, last_modified: identity.last_modified, parent_uid: options.parentUid, item_uid: options.versionOf?.uid, base_revision: options.versionOf?.revision, file_name: file.name, mime_type: file.type || 'application/octet-stream', size: file.size }), retrySafe: true, timeoutMs: 60_000, signal });
  const operation_uid = state.operation_uid || identity.operation_uid;
  if (state.completed && state.item) { pageOperations.delete(identity.key); progress('finalizing', file.size); return state.item; }
  const partSize = state.part_size;
  if (!Number.isSafeInteger(partSize) || !partSize || partSize <= 0 || partSize > 50 * 1024 * 1024) throw new Error('MX returned an invalid multipart size.');
  const parts = Math.max(1, Math.ceil(file.size / partSize)); const present = new Set(state.uploaded_parts || []);
  if ([...present].some((index) => !Number.isSafeInteger(index) || index < 0 || index >= parts)) throw new Error('MX returned an invalid stored part index.');
  let loaded = Array.from(present).reduce((size, index) => size + Math.max(0, Math.min(partSize, file.size - index * partSize)), 0);
  let nextIndex = state.ready_to_finalize ? parts : 0;
  if (state.ready_to_finalize) loaded = file.size;
  progress('uploading', loaded, present.size);
  const meter = new UploadMeter(file.size, loaded);
  let lastEmission = 0;
  const emit = (force = false) => { if (force || performance.now() - lastEmission >= 200) { lastEmission = performance.now(); onProgress?.({ phase: 'uploading', total: file.size, resumedParts: present.size, ...meter.snapshot() }); } };
  const timer = setInterval(() => emit(), 1000);
  // Console's default 1/2/3/4 workers; two queued files never exceed eight parts.
  let stopped = false;
  const worker = async () => {
    while (!stopped && nextIndex < parts) {
      signal?.throwIfAborted(); const index = nextIndex++; if (present.has(index)) continue;
      const chunk = file.slice(index * partSize, Math.min(file.size, (index + 1) * partSize));
      try { await apiUploadPart(`${root}/uploads/${operation_uid}/parts/${index}`, chunk, (bytes) => { meter.part(index, bytes, chunk.size); emit(); }, signal); }
      catch (reason) { stopped = true; throw reason; }
      loaded += chunk.size; meter.stored(index, chunk.size); emit(true);
    }
  };
  const workers = Math.max(1, Math.min(4, state.workers || 2));
  let results: PromiseSettledResult<void>[];
  try { results = await Promise.allSettled(Array.from({ length: workers }, () => worker())); }
  finally { clearInterval(timer); }
  const failed = results.find((result) => result.status === 'rejected');
  if (failed?.status === 'rejected') throw failed.reason;
  progress('finalizing', loaded, present.size);
  const item = await apiJson<DriveItem>(`${root}/uploads/${operation_uid}/finish`, { method: 'POST', retrySafe: true, timeoutMs: 180_000, signal });
  pageOperations.delete(identity.key); return item;
}

export async function uploadDriveBatch(entries: { file: File; parentUid: string | null; versionOf?: DriveItem }[], accountUid: string, signal: AbortSignal, onProgress?: (loaded: number, total: number) => void) {
  const identities = await Promise.all(entries.map((entry) => uploadIdentity(entry.file, { accountUid, parentUid: entry.parentUid, versionOf: entry.versionOf, signal })));
  const metadata = entries.map((entry, index) => ({ operation_uid: identities[index].operation_uid, parent_uid: entry.parentUid, item_uid: entry.versionOf?.uid, base_revision: entry.versionOf?.revision }));
  const body = new FormData(); body.append('metadata', JSON.stringify(metadata));
  for (const entry of entries) body.append('file', entry.file, entry.file.name);
  const response = await apiUpload<{ results: { operation_uid: string; status: number; item?: DriveItem; error?: string }[] }>(`${root}/batch`, body, onProgress, true, signal);
  return metadata.map((entry, index) => {
    const result = response.results.find((result) => result.operation_uid === entry.operation_uid) || { operation_uid: entry.operation_uid, status: 502, error: 'This file was not acknowledged. Retry the same file to reconcile it.' };
    if (result.status < 300 && result.item) pageOperations.delete(identities[index].key);
    return result;
  });
}

export interface GuestDrivePage { root: DriveItem; item: DriveItem; items: DriveItem[]; breadcrumbs: DriveItem[]; has_more: boolean; total: number; offset: number; limit: number }
export const guestDrive = (token: string, uid?: string, offset = 0, limit=50) => apiJson<GuestDrivePage>(`${root}/guest/${encodeURIComponent(token)}?${new URLSearchParams({ ...(uid ? { item_uid: uid } : {}), offset: String(offset),limit:String(limit) })}`, {}, false);
export const guestNeighbors = (token:string,uid:string,signal?:AbortSignal) => apiJson<DriveNeighbors>(`${root}/guest/${encodeURIComponent(token)}/neighbors?${new URLSearchParams({item_uid:uid})}`,{signal},false);
export function guestPreview(token: string, item: DriveItem) {
  const office = /\.(docx?|xlsx?|pptx?|odt|ods|odp|rtf)$/i.test(item.name) || /(?:msword|ms-excel|ms-powerpoint|officedocument|opendocument|rtf)/i.test(item.mime_type);
  return { url: `${root}/guest/${encodeURIComponent(token)}/media/${item.uid}?revision=${item.revision}${office ? '&office=true' : ''}`, fileName: office ? `${item.name}.pdf` : item.name, mimeType: office ? 'application/pdf' : item.mime_type };
}
export const guestDownloadUrl = (token: string, item: DriveItem) => `${root}/guest/${encodeURIComponent(token)}/media/${item.uid}?download=true&revision=${item.revision}`;
export function driveSize(value: number): string { if (value < 1024) return `${value} B`; const unit = Math.min(3, Math.floor(Math.log(value) / Math.log(1024))); return `${(value / 1024 ** unit).toFixed(unit > 1 ? 1 : 0)} ${['B', 'KB', 'MB', 'GB'][unit]}`; }
