import type { DriveItem } from '../api/drive';

export type UploadVersion = Pick<DriveItem, 'uid' | 'revision' | 'parent_uid'>;
export interface UploadRecovery {
  uid: string; operationUid: string; name: string; size: number; mimeType: string;
  lastModified: number; relativePath: string; parent: string | null;
  version?: UploadVersion; storedBytes: number; manuallyPaused: boolean;
}
const prefix = (account: string) => `mx:drive:pending:${account}:`;
const uuid = (value: unknown): value is string => typeof value === 'string' && /^[a-f0-9-]{36}$/i.test(value);
const integer = (value: unknown): value is number => Number.isSafeInteger(value) && Number(value) >= 0;
export function readUploadRecovery(account: string): UploadRecovery[] {
  try {
    const value: unknown[] = [];
    for (let index = 0; index < localStorage.length; index++) {
      const key = localStorage.key(index);
      if (key?.startsWith(prefix(account))) {
        try { value.push(JSON.parse(localStorage.getItem(key) || 'null')); } catch { /* One corrupt entry must not hide other uploads. */ }
      }
    }
    return value.filter((candidate): candidate is UploadRecovery => {
      if (!candidate || typeof candidate !== 'object') return false;
      const entry = candidate as Partial<UploadRecovery>;
      return uuid(entry.uid) && uuid(entry.operationUid) && typeof entry.name === 'string' && entry.name.length <= 1024 &&
        typeof entry.mimeType === 'string' && typeof entry.relativePath === 'string' &&
        integer(entry.size) && integer(entry.lastModified) && integer(entry.storedBytes) && entry.storedBytes <= entry.size &&
        (entry.parent === null || uuid(entry.parent)) && typeof entry.manuallyPaused === 'boolean' &&
        (!entry.version || uuid(entry.version.uid) && integer(entry.version.revision) && (entry.version.parent_uid === null || uuid(entry.version.parent_uid)));
    });
  } catch { return []; }
}
/** Metadata only. Never serialize File/Blob bytes, credentials or N1 tokens. */
export function writeUploadRecovery(account: string, entries: UploadRecovery[], removed: string[] = []): boolean {
  try {
    // One atomic key per transfer, so a second tab cannot erase an unrelated
    // upload's recovery entry by writing its own queue. Write changed data only.
    for (const entry of entries) {
      const key = `${prefix(account)}${entry.uid}`, serialized = JSON.stringify(entry);
      if (localStorage.getItem(key) !== serialized) localStorage.setItem(key, serialized);
    }
    for (const uid of removed) localStorage.removeItem(`${prefix(account)}${uid}`);
    return true;
  } catch { return false; }
}
