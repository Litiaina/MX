import type { MxNotification } from '../api/domain';

export type NotificationCategory =
  | 'message'
  | 'mention'
  | 'file'
  | 'collaboration'
  | 'record-created'
  | 'record-updated'
  | 'record-deleted'
  | 'assignment'
  | 'workflow'
  | 'task'
  | 'security'
  | 'system';

export interface NotificationPresentation {
  category: NotificationCategory;
  label: string;
}

export function notificationPresentation(item: Pick<MxNotification, 'kind' | 'target_type'>): NotificationPresentation {
  const kind = item.kind.toLocaleLowerCase();
  if (kind.startsWith('mention.')) return { category: 'mention', label: 'Mention' };
  if (kind.startsWith('message.')) return { category: 'message', label: 'Message' };
  if (kind.startsWith('file.') || kind.startsWith('attachment.')) return { category: 'file', label: 'File' };
  if (kind === 'record.created') return { category: 'record-created', label: 'Record created' };
  if (kind === 'record.deleted') return { category: 'record-deleted', label: 'Record deleted' };
  if (kind === 'record.assigned') return { category: 'assignment', label: 'Assignment' };
  if (kind.startsWith('record.')) return { category: 'record-updated', label: kind === 'record.restored' ? 'Record restored' : 'Record change' };
  if (kind.startsWith('workflow.')) return { category: 'workflow', label: 'Workflow' };
  if (kind.startsWith('task.')) return { category: 'task', label: 'Task' };
  if (kind.startsWith('channel.') || item.target_type === 'channel') return { category: 'collaboration', label: 'Collaboration' };
  if (kind.startsWith('security.') || kind.startsWith('auth.') || kind.startsWith('account.')) return { category: 'security', label: 'Security' };
  return { category: 'system', label: 'System' };
}
