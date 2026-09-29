import { loadProfilePhoto } from '../api/account';

const photoUrls = new Map<string, Promise<string>>();

export function profilePhotoUrl(userUid: string, updatedAt: number): Promise<string> {
  const key = `${userUid}:${updatedAt}`;
  let pending = photoUrls.get(key);
  if (!pending) {
    pending = loadProfilePhoto(userUid, updatedAt).then((blob) => URL.createObjectURL(blob));
    photoUrls.set(key, pending);
    pending.catch(() => photoUrls.delete(key));
  }
  return pending;
}

export function clearProfilePhotoCache(userUid: string): void {
  for (const [key, pending] of photoUrls) {
    if (!key.startsWith(`${userUid}:`)) continue;
    photoUrls.delete(key);
    void pending.then((url) => URL.revokeObjectURL(url)).catch(() => undefined);
  }
}
