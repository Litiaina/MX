import type { DriveItem } from '../api/drive';
export interface OfficeDraft {
  key: string; accountUid: string; item: DriveItem; extension: string;
  blob: Blob; generation: number; acknowledgedGeneration: number; updatedAt: number;
  saveFile?: File;
  saveOperation?: string;
  saveGeneration?: number;
  copyFile?: File;
  copyOperation?: string;
  copyGeneration?: number;
}
const databaseName = 'mx-office-local-drafts';
async function database(): Promise<IDBDatabase> {
  if (!globalThis.indexedDB) throw new Error('This browser does not support persistent offline drafts. Download a copy before closing.');
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(databaseName, 1);
    request.onupgradeneeded = () => request.result.createObjectStore('drafts', { keyPath: 'key' });
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(new Error('Offline draft storage is unavailable. Download a copy before closing.'));
  });
}
async function transaction<T>(mode: IDBTransactionMode, operation: (store: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  const db = await database();
  try {
    return await new Promise<T>((resolve, reject) => {
      const tx = db.transaction('drafts', mode); const request = operation(tx.objectStore('drafts'));
      tx.oncomplete = () => resolve(request.result);
      tx.onabort = () => reject(new Error('Offline draft could not be stored. Check available browser storage and download a copy.'));
      tx.onerror = () => reject(new Error('Offline draft storage failed. Download a copy.'));
    });
  } finally { db.close(); }
}
export const draftKey = (accountUid: string, itemUid: string) => `${accountUid}:${itemUid}`;
export const storeDraft = (draft: OfficeDraft) => transaction('readwrite', (store) => store.put(draft));
export const removeDraft = (key: string) => transaction('readwrite', (store) => store.delete(key));
export async function listDrafts(): Promise<OfficeDraft[]> { return (await transaction('readonly', (store) => store.getAll()) as OfficeDraft[]).sort((a,b) => b.updatedAt-a.updatedAt); }
