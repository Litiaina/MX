import { describe, expect, it } from 'vitest';
import { recordNotificationIntent, shouldSendChatMessage } from './interactions';

const key = (changes: Partial<Parameters<typeof shouldSendChatMessage>[0]> = {}) => ({
  key: 'Enter', shiftKey: false, ctrlKey: false, metaKey: false, altKey: false, isComposing: false, ...changes
});

describe('chat composer keyboard behavior', () => {
  it('sends with bare Enter', () => expect(shouldSendChatMessage(key())).toBe(true));
  it('keeps Shift + Enter as a new line', () => expect(shouldSendChatMessage(key({ shiftKey: true }))).toBe(false));
  it('does not hijack modified Enter or IME composition', () => {
    expect(shouldSendChatMessage(key({ ctrlKey: true }))).toBe(false);
    expect(shouldSendChatMessage(key({ metaKey: true }))).toBe(false);
    expect(shouldSendChatMessage(key({ isComposing: true }))).toBe(false);
  });
});

describe('record notification navigation', () => {
  it('focuses attachments for attachment notifications', () => {
    expect(recordNotificationIntent({ kind: 'attachment.created', target_type: 'record', target_uid: 'record-1', module_uid: 'module-1' }))
      .toEqual({ moduleUid: 'module-1', recordUid: 'record-1', focusAttachments: true });
  });

  it('opens ordinary record notifications without forcing attachment focus', () => {
    expect(recordNotificationIntent({ kind: 'record.updated', target_type: 'record', target_uid: 'record-1', module_uid: 'module-1' }))
      .toEqual({ moduleUid: 'module-1', recordUid: 'record-1', focusAttachments: false });
  });
});
