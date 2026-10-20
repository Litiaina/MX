// Inspect only the ZIP directory. Never decompress/scan the whole workbook or
// remove its formatting/data. This does not impose a Drive upload limit.
export interface OfficeResources { expandedBytes: number | null; large: boolean }
// This engine has a fixed 1-GiB heap. Do not buffer multi-gigabyte originals
// into it; Drive storage/uploads and streamed downloads remain unrestricted.
export const OFFICE_INPUT_BUDGET = 128 * 1024 ** 2;
export function checkOfficeInputSize(size: number, source: 'drive' | 'chat' | 'record' = 'drive'): void {
  if (size > OFFICE_INPUT_BUDGET && source !== 'drive') throw new Error(`This ${source} attachment exceeds the browser viewer’s 128 MiB loading budget. The shared original is unchanged. Download it to open locally; this is an editor memory limit, not an upload limit.`);
  if (size > OFFICE_INPUT_BUDGET) throw new Error('This file exceeds the browser editor’s 128 MiB loading budget. It is stored safely in Drive. Download the original to edit locally; this is not a Drive upload limit.');
}
export async function officeResources(blob: Blob, extension: string): Promise<OfficeResources> {
  let expandedBytes: number | null = null;
  if (['.xlsx', '.ods', '.docx', '.odt', '.pptx', '.odp'].includes(extension)) {
    const tailStart = Math.max(0, blob.size - 65557);
    const tail = new DataView(await blob.slice(tailStart).arrayBuffer());
    for (let offset = tail.byteLength - 22; offset >= 0; offset--) {
      if (tail.getUint32(offset, true) !== 0x06054b50 || offset + 22 + tail.getUint16(offset + 20, true) !== tail.byteLength) continue;
      const count = tail.getUint16(offset + 10, true), length = tail.getUint32(offset + 12, true), start = tail.getUint32(offset + 16, true);
      if (count > 10000 || length > 2 * 1024 ** 2 || start + length > tailStart + offset) break;
      const directory = new DataView(await blob.slice(start, start + length).arrayBuffer());
      let at = 0, total = 0, valid = true;
      for (let entry = 0; entry < count; entry++) {
        if (at + 46 > directory.byteLength || directory.getUint32(at, true) !== 0x02014b50) { valid = false; break; }
        const size = directory.getUint32(at + 24, true);
        if (size === 0xffffffff) { valid = false; break; } // ZIP64: size fallback.
        total += size;
        at += 46 + directory.getUint16(at + 28, true) + directory.getUint16(at + 30, true) + directory.getUint16(at + 32, true);
        if (at > directory.byteLength) { valid = false; break; }
      }
      if (valid) expandedBytes = total;
      break;
    }
  }
  const large = expandedBytes !== null ? expandedBytes > 24 * 1024 ** 2 : blob.size > 8 * 1024 ** 2;
  if (expandedBytes !== null && expandedBytes > 256 * 1024 ** 2) throw new Error('This document expands beyond the bundled editor’s loading budget. Your original is unchanged. Download it to edit locally, or split a working copy into smaller documents.');
  return { expandedBytes, large };
}
export const OFFICE_STARTUP_TIMEOUT = 10 * 60_000;
export function officeOperationTimeout(command: string): number {
  return ['open', 'export'].includes(command) ? 10 * 60_000 : 30_000;
}
export function officeTimeoutMessage(command: string): string {
  if (command === 'open') return 'This document did not finish opening within 10 minutes. No changes were saved. Download the original, retry, or return to Drive.';
  if (command === 'export') return 'The editor did not finish preparing your edited file within 10 minutes. MX has not confirmed a save. Keep this tab open; your edits may still be in the editor.';
  return 'The editor is not responding to this action. No save is confirmed by this message.';
}
