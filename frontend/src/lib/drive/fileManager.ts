export interface UploadEntry { file: File; relativePath: string }
export interface DropFiles { files: UploadEntry[]; folders: string[] }
interface Entry {
  isFile: boolean; isDirectory: boolean; name: string;
  file?: (ok: (file: File) => void, fail: (error: DOMException) => void) => void;
  createReader?: () => { readEntries: (ok: (entries: Entry[]) => void, fail: (error: DOMException) => void) => void };
}

/** Snapshot drag items synchronously: browsers clear DataTransfer after drop. */
export async function droppedFiles(data: DataTransfer): Promise<DropFiles> {
  const entries = Array.from(data.items || []).filter(i => i.kind === 'file').map(i => {
    const item = i as DataTransferItem & { getAsEntry?: () => Entry | null; webkitGetAsEntry?: () => Entry | null };
    const file=i.getAsFile();let entry: Entry|null|undefined;
    try { entry=item.webkitGetAsEntry?.() || item.getAsEntry?.(); }
    catch { if(!file) throw new Error('This browser cannot read the dropped folder. Use Upload folder instead.'); }
    return {entry,file};
  });
  const fallback = Array.from(data.files || []);
  const result: DropFiles = { files: [], folders: [] };
  const visit = async (entry: Entry, prefix: string) => {
    const path = prefix + entry.name;
    if (entry.isFile && entry.file) {
      const file = await new Promise<File>((ok, fail) => entry.file!(ok, fail));
      result.files.push({ file, relativePath: path });
    } else if (entry.isDirectory && entry.createReader) {
      result.folders.push(path);
      const reader = entry.createReader();
      for (;;) {
        const children = await new Promise<Entry[]>((ok, fail) => reader.readEntries(ok, fail));
        if (!children.length) break;
        for (const child of children) await visit(child, `${path}/`);
      }
    } else throw new Error('This browser cannot read the dropped folder. Use Upload folder instead.');
    if (result.files.length + result.folders.length > 2000 || result.files.length > 1000) throw new Error('Drop up to 1,000 files and 1,000 folders at a time.');
  };
  if (entries.length) for (const { entry, file } of entries) {
    if (entry?.isDirectory) await visit(entry, '');
    // Use the snapshotted File directly for ordinary drops. WebKit can expose
    // an entry whose file() fails even though getAsFile() is already usable.
    else if (file) result.files.push({ file, relativePath: file.webkitRelativePath || file.name });
    else if (entry) await visit(entry, '');
    else throw new Error('This browser cannot read the dropped folder. Use Upload folder instead.');
  }
  else result.files = fallback.map(file => ({ file, relativePath: file.webkitRelativePath || file.name }));
  return result;
}

export function selection(current: Set<string>, ordered: string[], uid: string, anchor: string | null, toggle: boolean, range: boolean): Set<string> {
  if (range && anchor && ordered.includes(anchor)) {
    const a = ordered.indexOf(anchor), b = ordered.indexOf(uid);
    return new Set([...(toggle ? current : []), ...ordered.slice(Math.min(a,b), Math.max(a,b)+1)]);
  }
  if (toggle) { const next = new Set(current); if (!next.delete(uid)) next.add(uid); return next; }
  return new Set([uid]);
}
export function intersects(a: {left:number;top:number;right:number;bottom:number}, b: {left:number;top:number;right:number;bottom:number}) {
  return a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top;
}
export function isEditing(target: EventTarget | null) {
  return target instanceof Element && !!target.closest('textarea,select,[contenteditable="true"],[role="textbox"],input:not([type="checkbox"]):not([type="radio"]):not([type="button"]):not([type="submit"])');
}
export function activityLabel(action: string) {
  return ({'folder.created':'Created this folder','file.created':'Uploaded this file','file.version':'Uploaded a new version','file.uploaded':'Uploaded this file','version.created':'Uploaded a new version','version.uploaded':'Uploaded a new version','file.version_restored':'Restored an earlier version','item.rename':'Renamed this item','item.move':'Moved this item','item.trash':'Moved this item to trash','item.restore':'Restored this item from trash','item.copied':'Created a copy','sharing.changed':'Updated sharing access','sharing.grant':'Updated sharing access','sharing.revoke':'Removed sharing access','public_link.created':'Created a guest link','public_link.revoked':'Revoked a guest link','link.created':'Created a guest link','link.revoked':'Revoked a guest link'} as Record<string,string>)[action] || action.replaceAll(/[._]/g,' ');
}
