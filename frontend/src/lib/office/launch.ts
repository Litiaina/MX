import type { DriveItem } from '../api/drive';
import { officeExtension, officePresentation } from './formats';
export function openMessageOffice(fileUid: string): boolean {
  sessionStorage.setItem('mx_office_theme',document.documentElement.dataset.theme || 'light');
  // Older Office service workers cache index.html even while online. A new
  // public entry path cannot accidentally load that pre-attachment shell.
  return !!window.open(`/office/attachment.html#message_file=${encodeURIComponent(fileUid)}`, '_blank');
}
export function openRecordOffice(recordUid: string, attachmentUid: string, moduleUid: string): boolean {
  sessionStorage.setItem('mx_office_theme',document.documentElement.dataset.theme || 'light');
  const params = new URLSearchParams({record:recordUid,attachment:attachmentUid,module:moduleUid});
  return !!window.open(`/office/attachment.html#${params}`, '_blank');
}
export const canOpenOffice = (item: DriveItem) => item.kind === 'file' && !item.trashed_at && !!officeExtension(item.original_file_name || item.name);
export const canEditOffice = (item: DriveItem) => canOpenOffice(item) && !['viewer','guest'].includes(item.permission) && !officePresentation(officeExtension(item.original_file_name || item.name));
export function openOffice(item: DriveItem): boolean {
  sessionStorage.setItem('mx_office_theme',document.documentElement.dataset.theme || 'light');
  // No bearer tokens or file contents in URLs. Same-origin sessionStorage is
  // copied to the new tab by the browser; COOP isolates it from the workspace.
  return !!window.open(`/office/index.html#file=${encodeURIComponent(item.uid)}`, '_blank');
}
