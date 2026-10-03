import type { MxNotification } from '../api/domain';

export interface ComposerKeyIntent {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  isComposing: boolean;
}

export function shouldSendChatMessage(event: ComposerKeyIntent) {
  return event.key === 'Enter'
    && !event.shiftKey
    && !event.ctrlKey
    && !event.metaKey
    && !event.altKey
    && !event.isComposing;
}

export function recordNotificationIntent(item: Pick<MxNotification, 'kind' | 'target_type' | 'target_uid' | 'module_uid'>) {
  if (item.target_type !== 'record' || !item.module_uid || !item.target_uid) return null;
  return {
    moduleUid: item.module_uid,
    recordUid: item.target_uid,
    focusAttachments: item.kind.startsWith('attachment.')
  };
}
