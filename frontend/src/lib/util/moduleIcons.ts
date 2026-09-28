export const MODULE_ICON_OPTIONS = [
  { value: 'file-text', label: 'Document' },
  { value: 'folder', label: 'Folder' },
  { value: 'inbox', label: 'Inbox' },
  { value: 'clipboard-list', label: 'Checklist' },
  { value: 'database', label: 'Database' },
  { value: 'users', label: 'People' },
  { value: 'building-2', label: 'Office' },
  { value: 'briefcase-business', label: 'Work' },
  { value: 'calendar-days', label: 'Calendar' },
  { value: 'package', label: 'Inventory' },
  { value: 'landmark', label: 'Institution' },
  { value: 'scale', label: 'Legal' },
  { value: 'heart-pulse', label: 'Health' },
  { value: 'graduation-cap', label: 'Education' },
  { value: 'shield-check', label: 'Compliance' },
  { value: 'wrench', label: 'Operations' },
] as const;

export function normaliseModuleIcon(value: string | null | undefined) {
  const icon = value?.trim().toLowerCase();
  if (!icon || icon === '▤' || !MODULE_ICON_OPTIONS.some((option) => option.value === icon)) return 'file-text';
  return icon;
}
