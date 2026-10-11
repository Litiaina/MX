import { driveCancelUpload, driveFolder, driveList, uploadDriveBatch, uploadDriveFile, type DriveItem, type UploadProgress } from '../api/drive';
import { UploadMeter } from '../util/uploadProgress';
import type { UploadEntry } from './fileManager';
import { ApiError, ConnectionError } from '../api/client';
import { readUploadRecovery, writeUploadRecovery, type UploadRecovery } from './uploadRecovery';

export interface DriveTransfer extends Omit<UploadRecovery, 'storedBytes'> {
  file: File | null;
  status: 'queued' | 'uploading' | 'waiting' | 'needs_file' | 'paused' | 'failed' | 'done';
  progress: UploadProgress; error: string; retryCount: number; retryAt: number; controller?: AbortController;
}
const transient = (reason: unknown) => reason instanceof ConnectionError || reason instanceof ApiError && [408, 429, 500, 502, 503, 504].includes(reason.status);
const initialProgress = (size: number, storedBytes = 0): UploadProgress => ({ loaded: storedBytes, storedBytes, total: size, phase: 'initializing', resumedParts: 0, bytesPerSecond: 0, etaSeconds: null });

/** One queue per authenticated workspace, NOT per mounted Drive page. */
export function createDriveUploads(accountUid: string) {
  let transfers = $state<DriveTransfer[]>(readUploadRecovery(accountUid).map(entry => ({ ...entry, file: null, status: 'needs_file', progress: initialProgress(entry.size, entry.storedBytes), error: '', retryAt: 0, retryCount: 0 })));
  let preparing = $state(0);
  let revision = $state(0);
  let collapsed = $state(false);
  let preparationError = $state('');
  let clipboard = $state<{ items: DriveItem[]; action: 'copy' | 'move'; operation: string } | null>(null);
  let recoveryAvailable = $state(true);
  let workers = 0; let disposed = false;
  const removedRecovery = new Set<string>();
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  let persistTimer: ReturnType<typeof setTimeout> | undefined;
  const offline = () => typeof navigator !== 'undefined' && navigator.onLine === false;
  const active = () => transfers.some((item) => ['uploading', 'queued', 'waiting'].includes(item.status)) || preparing > 0;
  function persist() {
    clearTimeout(persistTimer); persistTimer = undefined;
    // Restored entries without a selected File belong to no running job here;
    // don't overwrite their progress if another tab is still uploading them.
    recoveryAvailable = writeUploadRecovery(accountUid, transfers.filter(item => item.file && item.status !== 'done').map(item => ({
      uid: item.uid, operationUid: item.operationUid, name: item.name, size: item.size, mimeType: item.mimeType,
      lastModified: item.lastModified, relativePath: item.relativePath, parent: item.parent, version: item.version,
      storedBytes: item.progress.storedBytes || 0, manuallyPaused: item.manuallyPaused
    })), [...removedRecovery, ...transfers.filter(item => item.status === 'done').map(item => item.uid)]);
    if (recoveryAvailable) removedRecovery.clear();
  }
  function checkpoint() { if (!persistTimer) persistTimer = setTimeout(persist, 1000); }
  function scheduleRetry() {
    clearTimeout(retryTimer); retryTimer = undefined;
    if (disposed || offline()) return;
    const waiting = transfers.filter(item => item.status === 'waiting');
    if (!waiting.length) return;
    retryTimer = setTimeout(() => {
      retryTimer = undefined;
      for (const item of waiting) if (item.status === 'waiting' && item.retryAt <= Date.now()) item.status = 'queued';
      kick(); scheduleRetry();
    }, Math.max(0, Math.min(...waiting.map(item => item.retryAt)) - Date.now()));
  }
  function interrupted(item: DriveTransfer, reason: unknown) {
    item.progress = { ...item.progress, loaded: item.progress.storedBytes || 0, bytesPerSecond: 0, etaSeconds: null };
    item.error = reason instanceof Error ? reason.message : 'Upload interrupted.';
    if (reason instanceof ApiError && reason.status === 422 || reason instanceof DOMException && ['NotFoundError', 'NotReadableError'].includes(reason.name)) {
      item.status = 'needs_file';
      persist(); // Save acknowledged progress before releasing a broken File handle.
      item.file = null;
      item.error = 'Your browser could no longer read the selected file. Select the original file to continue; completed parts are retained.';
    } else if (item.manuallyPaused) item.status = 'paused';
    else if (transient(reason) || offline() && !(reason instanceof ApiError)) {
      item.status = 'waiting'; item.retryCount++;
      item.retryAt = Date.now() + Math.min(30_000, 2000 * 2 ** Math.min(4, item.retryCount - 1)) + Math.random() * 500;
    } else item.status = 'failed';
    persist(); scheduleRetry();
  }
  function networkOffline() {
    for (const item of transfers) {
      if (item.status === 'queued') interrupted(item, new ConnectionError('Connection interrupted.'));
      else if (item.status === 'uploading') item.controller?.abort(new ConnectionError('Connection interrupted.'));
    }
    scheduleRetry();
  }
  function networkOnline() {
    for (const item of transfers) if (item.status === 'waiting') item.retryAt = Date.now();
    scheduleRetry(); // The actual resumable init checks MX AND N1, not just navigator.onLine.
  }
  if (typeof window !== 'undefined') { window.addEventListener('offline', networkOffline); window.addEventListener('online', networkOnline); window.addEventListener('pagehide', persist); }
  function kick() { while (!disposed && workers < 2 && transfers.some((item) => item.status === 'queued')) { workers++; void work(); } }
  async function work() {
    try { while (!disposed) {
      if (offline()) { for (const item of transfers.filter(item => item.status === 'queued')) interrupted(item, new ConnectionError('Connection interrupted.')); break; }
      const batch = transfers.filter((item) => item.status === 'queued' && item.file && item.size <= 1024 * 1024).slice(0, 16);
      // A retried single member of a small-file batch must keep the same protocol.
      if (batch.length) {
        const controller = new AbortController();
        for (const item of batch) { item.status = 'uploading'; item.error = ''; item.controller = controller; }
        const meters = batch.map((item) => new UploadMeter(item.size)); let lastEmission = 0;
        try {
          const results = await uploadDriveBatch(batch.map((item) => ({ file: item.file!, parentUid: item.parent, versionOf: item.version, operationUid: item.operationUid })), accountUid, controller.signal, (loaded, total) => {
            if (disposed) return;
            batch.forEach((item, index) => meters[index].part(0, Math.round(item.size * loaded / Math.max(1, total)), item.size));
            if (performance.now() - lastEmission < 200 && loaded !== total) return;
            lastEmission = performance.now();
            batch.forEach((item, index) => item.progress = { phase: 'uploading', total: item.size, storedBytes: 0, resumedParts: 0, ...meters[index].snapshot() });
          });
          if (disposed) return;
          batch.forEach((item, index) => {
            if (results[index].status < 300 && results[index].item) item.status = 'done';
            else interrupted(item, new ApiError(results[index].error || 'Upload not acknowledged.', results[index].status, null));
          });
          revision++;
        } catch (reason) { if (!disposed) for (const item of batch) interrupted(item, reason); }
        finally { for (const item of batch) item.controller = undefined; if (!disposed) { persist(); scheduleRetry(); } }
        continue;
      }
      const transfer = transfers.find((item) => item.status === 'queued'); if (!transfer) break;
      if (!transfer.file) { transfer.status = 'needs_file'; continue; }
      transfer.status = 'uploading'; transfer.error = ''; const controller = new AbortController(); transfer.controller = controller;
      try {
        await uploadDriveFile(transfer.file, { accountUid, parentUid: transfer.parent, versionOf: transfer.version, operationUid: transfer.operationUid, previousStoredBytes: transfer.progress.storedBytes, signal: controller.signal,
          onOperation: uid => { if (!disposed) { transfer.operationUid = uid; persist(); } },
          onProgress: value => { if (!disposed) { transfer.progress = value; checkpoint(); } } });
        if (disposed) return;
        transfer.status = 'done'; revision++;
      } catch (reason) { if (!disposed) interrupted(transfer, reason); }
      finally { transfer.controller = undefined; if (!disposed) { persist(); scheduleRetry(); } }
    } } finally {
      workers--;
      if (!preparing && transfers.length && transfers.every((item) => item.status === 'done')) collapsed = true;
      kick();
    }
  }
  return {
    accountUid,
    get transfers() { return transfers; }, get preparing() { return preparing > 0; }, get revision() { return revision; },
    get collapsed() { return collapsed; }, get preparationError() { return preparationError; },
    get clipboard() { return clipboard; },
    get recoveryAvailable() { return recoveryAvailable; },
    stage(items: DriveItem[], action: 'copy' | 'move') { clipboard = {items: items.map(item => ({...item})), action, operation: crypto.randomUUID()}; },
    clearClipboard() { clipboard = null; },
    toggle() { collapsed = !collapsed; }, clearCompleted() { transfers = transfers.filter((item) => item.status !== 'done'); },
    clearError() { preparationError = ''; }, active,
    pause(item: DriveTransfer) {
      // A small-file group shares one request. Keep every affected entry manually paused.
      for (const affected of transfers.filter(entry => entry === item || item.controller && entry.controller === item.controller)) {
        affected.manuallyPaused = true;
        if (affected.status !== 'uploading') affected.status = affected.file ? 'paused' : 'needs_file';
      }
      item.controller?.abort(); persist(); scheduleRetry();
    },
    resume(item: DriveTransfer) { if (!item.file || !['failed', 'paused', 'waiting'].includes(item.status)) return; item.manuallyPaused = false; item.retryCount = 0; item.status = 'queued'; item.error = ''; persist(); scheduleRetry(); kick(); },
    reselect(item: DriveTransfer, file: File) {
      if (file.name !== item.name || file.size !== item.size || file.lastModified !== item.lastModified || (file.type || 'application/octet-stream') !== item.mimeType) {
        item.error = 'Choose the original file: name, size, type and modification time must match.'; return false;
      }
      item.file = file; item.error = ''; item.status = item.manuallyPaused ? 'paused' : 'queued'; persist(); kick(); return true;
    },
    async cancel(item: DriveTransfer) {
      if (!['needs_file', 'paused', 'failed'].includes(item.status) || item.controller) return;
      try { await driveCancelUpload(item.operationUid); removedRecovery.add(item.uid); transfers = transfers.filter(entry => entry !== item); persist(); }
      catch (reason) { if (item.file) item.status = 'failed'; item.error = reason instanceof Error ? reason.message : 'Cancellation was not acknowledged. Retry when MX is available.'; }
    },
    async enqueue(list: FileList | File[] | UploadEntry[], parent: string | null, version?: DriveItem, folderPaths: string[] = []) {
      if (disposed) return;
      const files: UploadEntry[] = Array.from(list as ArrayLike<File | UploadEntry>).map(entry => entry instanceof File ? {file: entry, relativePath: entry.webkitRelativePath || entry.name} : entry); if (!files.length && !folderPaths.length) return;
      collapsed = false; preparationError = '';
      if (files.length > 1000 || folderPaths.length > 1000) { preparationError = 'Select up to 1,000 files and 1,000 folders in one batch.'; return; }
      preparing++;
      const folders = new Map<string, string>();
      const preparePath = async (parts: string[]) => {
        if (parts.length > 64 || parts.some(part => !part || part === '.' || part === '..' || /[\\\x00-\x1f]/.test(part))) throw new Error('The folder contains an invalid path.');
        let destination = version?.parent_uid ?? parent;
        if (!version) for (const name of parts) {
          const key = JSON.stringify([destination, name]); let uid = folders.get(key);
          if (!uid) {
            const existing = (await driveList('mine', destination, name)).items.find(item => item.kind === 'folder' && item.name.toLowerCase() === name.toLowerCase() && item.parent_uid === destination);
            if (disposed) return destination;
            uid = existing?.uid || (await driveFolder(name, destination)).uid; folders.set(key, uid); revision++;
          }
          destination = uid;
        }
        return destination;
      };
      try {
        for (const path of folderPaths) { if (disposed) break; await preparePath(path.split('/')); }
        for (const {file, relativePath} of files) {
          if (disposed) break;
          const parts = relativePath.split('/');
          if (parts.some((part) => !part || part === '.' || part === '..' || /[\\\x00-\x1f]/.test(part))) throw new Error('The folder contains an invalid path.');
          const destination = await preparePath(parts.slice(0, -1));
          if (disposed) break;
          const recovered = transfers.find(item => item.status === 'needs_file' && item.parent === destination && item.version?.uid === version?.uid && item.version?.revision === version?.revision && item.name === file.name && item.size === file.size && item.lastModified === file.lastModified && item.mimeType === (file.type || 'application/octet-stream'));
          if (recovered) { recovered.file = file; recovered.error = ''; recovered.status = recovered.manuallyPaused ? 'paused' : 'queued'; }
          else transfers.push({ uid: crypto.randomUUID(), operationUid: crypto.randomUUID(), file, name: file.name, size: file.size, lastModified: file.lastModified, mimeType: file.type || 'application/octet-stream', relativePath, parent: destination, version: version ? {uid: version.uid, revision: version.revision, parent_uid: version.parent_uid} : undefined, manuallyPaused: false, status: 'queued', progress: initialProgress(file.size), retryCount: 0, retryAt: 0, error: '' });
        }
      } catch (reason) { preparationError = reason instanceof Error ? reason.message : 'Could not prepare folder upload.'; }
      finally { preparing--; persist(); kick(); }
    },
    dispose() {
      disposed = true; persist(); clearTimeout(retryTimer); clearTimeout(persistTimer);
      if (typeof window !== 'undefined') { window.removeEventListener('offline', networkOffline); window.removeEventListener('online', networkOnline); window.removeEventListener('pagehide', persist); }
      for (const item of transfers) item.controller?.abort(); transfers = []; clipboard = null;
    }
  };
}
export type DriveUploads = ReturnType<typeof createDriveUploads>;
