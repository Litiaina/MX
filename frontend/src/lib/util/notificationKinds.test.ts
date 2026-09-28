import { describe, expect, it } from 'vitest';
import { notificationPresentation } from './notificationKinds';

describe('notificationPresentation', () => {
  it.each([
    ['message.created', 'channel', 'message', 'Message'],
    ['mention.created', 'channel', 'mention', 'Mention'],
    ['file.shared', 'channel', 'file', 'File'],
    ['record.created', 'record', 'record-created', 'Record created'],
    ['record.updated', 'record', 'record-updated', 'Record change'],
    ['record.deleted', 'record', 'record-deleted', 'Record deleted'],
    ['workflow.approved', 'record', 'workflow', 'Workflow'],
    ['security.password_changed', null, 'security', 'Security']
  ])('classifies %s', (kind, target_type, category, label) => {
    expect(notificationPresentation({ kind, target_type })).toEqual({ category, label });
  });
});
