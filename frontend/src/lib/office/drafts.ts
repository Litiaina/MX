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
  retainOffline?: boolean;
  expiresAt?: number;
}
const databaseName = 'mx-office-local-drafts';
async function database(): Promise<IDBDatabase> {
  if (!globalThis.indexedDB) throw new Error('This browser does not support persistent offline drafts. Download a copy before closing.');
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(databaseName, 2);
    request.onupgradeneeded = () => {
      const store=request.result.objectStoreNames.contains('drafts')?request.transaction!.objectStore('drafts'):request.result.createObjectStore('drafts', { keyPath: 'key' });
      if(!store.indexNames.contains('document'))store.createIndex('document',['accountUid','item.uid']);
      if(!store.indexNames.contains('account'))store.createIndex('account','accountUid');
    };
    request.onblocked = () => reject(new Error('Recovery storage is being upgraded. Close older MX Office tabs and retry.'));
    request.onsuccess = () => {request.result.onversionchange=()=>request.result.close();resolve(request.result);};
    request.onerror = () => reject(new Error('Offline draft storage is unavailable. Download a copy before closing.'));
  });
}
async function transaction<T>(mode: IDBTransactionMode, operation: (store: IDBObjectStore) => IDBRequest<T>): Promise<T> {
  const db = await database();
  try {
    return await new Promise<T>((resolve, reject) => {
      let tx:IDBTransaction;
      try { tx=mode==='readwrite'?db.transaction('drafts',mode,{durability:'strict'}):db.transaction('drafts',mode); }
      catch(reason){if(!(reason instanceof TypeError))throw reason;tx=db.transaction('drafts',mode);}
      const request = operation(tx.objectStore('drafts'));
      tx.oncomplete = () => resolve(request.result);
      tx.onabort = () => reject(new Error('Offline draft could not be stored. Check available browser storage and download a copy.'));
      tx.onerror = () => reject(new Error('Offline draft storage failed. Download a copy.'));
    });
  } finally { db.close(); }
}
export const draftKey = (accountUid: string, itemUid: string, editorUid?: string) => `${accountUid}:${itemUid}${editorUid?`:${editorUid}`:''}`;
export const storeDraft = (draft: OfficeDraft) => transaction('readwrite', (store) => store.put(draft));
export const removeDraft = (key: string) => transaction('readwrite', (store) => store.delete(key));
export async function listDrafts(accountUid?:string,itemUid?:string): Promise<OfficeDraft[]> {
  const drafts=await transaction('readonly',store=>accountUid&&itemUid?store.index('document').getAll([accountUid,itemUid]):accountUid?store.index('account').getAll(accountUid):store.getAll()) as OfficeDraft[];
  // Only automatic recovery copies expire. Explicit offline copies are retained.
  for(const draft of drafts)if(draft.expiresAt&&draft.expiresAt<Date.now()&&!draft.retainOffline)await removeDraft(draft.key);
  return drafts.filter(draft=>!draft.expiresAt||draft.expiresAt>=Date.now()||draft.retainOffline).sort((a,b)=>b.updatedAt-a.updatedAt);
}
