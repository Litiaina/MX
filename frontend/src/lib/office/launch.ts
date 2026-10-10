import type { DriveItem } from '../api/drive';
import { officeExtension, officePresentation } from './formats';
export const canOpenOffice = (item: DriveItem) => item.kind === 'file' && !item.trashed_at && !!officeExtension(item.original_file_name || item.name);
export const canEditOffice = (item: DriveItem) => canOpenOffice(item) && !['viewer','guest'].includes(item.permission) && !officePresentation(officeExtension(item.original_file_name || item.name));
export function openOffice(item: DriveItem): boolean {
  // No bearer tokens or file contents in URLs. Same-origin sessionStorage is
  // copied to the new tab by the browser; COOP isolates it from the workspace.
  return !!window.open(`/office/index.html#file=${encodeURIComponent(item.uid)}`, '_blank');
}
