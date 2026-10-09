import { driveFolder, driveList, uploadDriveBatch, uploadDriveFile, type DriveItem, type UploadProgress } from '../api/drive';
import { UploadMeter } from '../util/uploadProgress';
import type { UploadEntry } from './fileManager';

export interface DriveTransfer {
  uid: string; file: File; relativePath?: string; parent: string | null; version?: DriveItem;
  status: 'queued' | 'uploading' | 'paused' | 'failed' | 'done';
  progress: UploadProgress; error: string; controller?: AbortController;
}

/** One queue per authenticated workspace, NOT per mounted Drive page. */
export function createDriveUploads(accountUid: string) {
  let transfers = $state<DriveTransfer[]>([]);
  let preparing = $state(0);
  let revision = $state(0);
  let collapsed = $state(false);
  let preparationError = $state('');
  let clipboard = $state<{ items: DriveItem[]; action: 'copy' | 'move'; operation: string } | null>(null);
  let workers = 0; let disposed = false;
  const active = () => transfers.some((item) => item.status === 'uploading' || item.status === 'queued') || preparing > 0;
  function kick() { while (!disposed && workers < 2 && transfers.some((item) => item.status === 'queued')) { workers++; void work(); } }
  async function work() {
    try { while (!disposed) {
      const batch = transfers.filter((item) => item.status === 'queued' && item.file.size <= 1024 * 1024).slice(0, 16);
      // A retried single member of a small-file batch must keep the same protocol.
      if (batch.length) {
        const controller = new AbortController();
        for (const item of batch) { item.status = 'uploading'; item.error = ''; item.controller = controller; }
        const meters = batch.map((item) => new UploadMeter(item.file.size)); let lastEmission = 0;
        try {
          const results = await uploadDriveBatch(batch.map((item) => ({ file: item.file, parentUid: item.parent, versionOf: item.version })), accountUid, controller.signal, (loaded, total) => {
            batch.forEach((item, index) => meters[index].part(0, Math.round(item.file.size * loaded / Math.max(1, total)), item.file.size));
            if (performance.now() - lastEmission < 200 && loaded !== total) return;
            lastEmission = performance.now();
            batch.forEach((item, index) => item.progress = { phase: 'uploading', total: item.file.size, resumedParts: 0, ...meters[index].snapshot() });
          });
          batch.forEach((item, index) => { item.status = results[index].status < 300 && results[index].item ? 'done' : 'failed'; item.error = results[index].error || ''; });
          revision++;
        } catch (reason) { for (const item of batch) { item.status = controller.signal.aborted ? 'paused' : 'failed'; item.error = reason instanceof Error ? reason.message : 'Batch interrupted.'; } }
        finally { for (const item of batch) item.controller = undefined; }
        continue;
      }
      const transfer = transfers.find((item) => item.status === 'queued'); if (!transfer) break;
      transfer.status = 'uploading'; transfer.error = ''; const controller = new AbortController(); transfer.controller = controller;
      try {
        await uploadDriveFile(transfer.file, { accountUid, parentUid: transfer.parent, versionOf: transfer.version, signal: controller.signal, onProgress: (value) => transfer.progress = value });
        transfer.status = 'done'; revision++;
      } catch (reason) { transfer.status = controller.signal.aborted ? 'paused' : 'failed'; transfer.error = reason instanceof Error ? reason.message : 'Upload interrupted.'; }
      finally { transfer.controller = undefined; }
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
    stage(items: DriveItem[], action: 'copy' | 'move') { clipboard = {items: items.map(item => ({...item})), action, operation: crypto.randomUUID()}; },
    clearClipboard() { clipboard = null; },
    toggle() { collapsed = !collapsed; }, clearCompleted() { transfers = transfers.filter((item) => item.status !== 'done'); },
    clearError() { preparationError = ''; }, active,
    pause(item: DriveTransfer) { if (item.status === 'queued') item.status = 'paused'; else item.controller?.abort(); },
    resume(item: DriveTransfer) { if (item.status !== 'failed' && item.status !== 'paused') return; item.status = 'queued'; item.error = ''; kick(); },
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
          transfers.push({ uid: crypto.randomUUID(), file, relativePath, parent: destination, version, status: 'queued', progress: { loaded: 0, total: file.size, phase: 'initializing', resumedParts: 0, bytesPerSecond: 0, etaSeconds: null }, error: '' });
        }
      } catch (reason) { preparationError = reason instanceof Error ? reason.message : 'Could not prepare folder upload.'; }
      finally { preparing--; kick(); }
    },
    dispose() { disposed = true; for (const item of transfers) item.controller?.abort(); transfers = []; clipboard = null; }
  };
}
export type DriveUploads = ReturnType<typeof createDriveUploads>;
