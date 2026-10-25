/** Read browser-exposed files only. Never treat pasted paths/URLs as files. */
export function transferHasFiles(transfer: DataTransfer | null): boolean {
  return !!transfer && (Array.from(transfer.types || []).includes('Files') || Array.from(transfer.items || []).some(item => item.kind === 'file') || transfer.files.length > 0);
}

export function transferredFiles(transfer: DataTransfer): { files: File[]; directories: boolean } {
  const files: File[] = [];
  const itemCopies = new Map<string, number>();
  // items/files are two views of the same transfer, not two selections. Some
  // browsers recreate File wrappers with different lastModified timestamps.
  // Match occurrences only ACROSS these views; do not weaken pending-file
  // identity or discard distinct selections globally by ignoring timestamps.
  const transferKey = (file: File) => JSON.stringify([file.name, file.size, file.type]);
  let directories = false;
  const directoryNames = new Set<string>();
  for (const item of Array.from(transfer.items || [])) {
    if (item.kind !== 'file') continue;
    const entry = item.webkitGetAsEntry?.();
    if (entry?.isDirectory) { directories = true; directoryNames.add(entry.name); continue; }
    const file = item.getAsFile();
    if (file) {
      files.push(file);
      const key = transferKey(file);
      itemCopies.set(key, (itemCopies.get(key) || 0) + 1);
    }
  }
  // Some browsers expose files but no DataTransferItems on paste.
  for (const file of Array.from(transfer.files || [])) {
    if (directoryNames.has(file.name)) continue;
    const key = transferKey(file), copies = itemCopies.get(key) || 0;
    if (copies) itemCopies.set(key, copies - 1);
    else files.push(file);
  }
  return { files: uniqueFiles(files), directories };
}

export function uniqueFiles(files: File[], existing: File[] = []): File[] {
  const key = (file: File) => JSON.stringify([file.name, file.size, file.lastModified, file.type]);
  const seen = new Set(existing.map(key));
  return files.filter(file => { const id = key(file); if (seen.has(id)) return false; seen.add(id); return true; });
}

export function clipboardFiles(files: File[], timestamp: string): File[] {
  return files.map((file, index) => {
    if (!file.type.toLowerCase().startsWith('image/') || (file.name.trim() && !/^(?:image|clipboard)(?:\.[a-z0-9]+)?$/i.test(file.name.trim()))) return file;
    const type = file.type.toLowerCase();
    const extension = ({ 'image/jpeg': 'jpg', 'image/svg+xml': 'svg', 'image/tiff': 'tiff' } as Record<string, string>)[type] || type.split('/')[1]?.replace(/[^a-z0-9]/gi, '') || 'png';
    return new File([file], `Screenshot-${timestamp}${index ? `-${index + 1}` : ''}.${extension}`, { type: file.type, lastModified: file.lastModified });
  });
}
