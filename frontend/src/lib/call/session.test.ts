import { describe, expect, it } from 'vitest';
import type { CallParticipant } from '../api/domain';
import type { LiveMessage } from '../live/client';
import { callEventKey, callLeaveMatches, callPeerIdentities, callSessionWasReplaced, createCallSessionUid, historicalCallEventKeys } from './session';

const oldEvents: LiveMessage[] = [
  { type: 'call.participant.left', sequence: 80, at: 1, actor_uid: 'self', payload: { channel_uid: 'room', user_uid: 'self', session_uid: 'old-session' } },
  { type: 'call.signal', sequence: 81, at: 2, actor_uid: 'remote', payload: { channel_uid: 'room', kind: 'offer' } },
  { type: 'call.signal', sequence: 82, at: 3, actor_uid: 'remote', payload: { channel_uid: 'room', kind: 'ice' } }
];

describe('call join sessions', () => {
  it('marks every retained event as historical when the call UI remounts', () => {
    const handled = historicalCallEventKeys(oldEvents);
    expect(oldEvents.every((event) => handled.has(callEventKey(event)))).toBe(true);
    expect(handled.has(callEventKey({ ...oldEvents[0], sequence: 83 }))).toBe(false);
  });

  it('accepts only a leave event for the current join session', () => {
    expect(callLeaveMatches('new-session', 'old-session')).toBe(false);
    expect(callLeaveMatches('new-session', 'new-session')).toBe(true);
    expect(callLeaveMatches('new-session', '')).toBe(true);
  });

  it('creates UUID join identifiers and maps only remote peer sessions', () => {
    expect(createCallSessionUid()).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
    const participants: CallParticipant[] = [
      { user_uid: 'self', session_uid: 'self-session', user_name: 'Self', audio_enabled: true, video_enabled: false, screen_sharing: false, joined_at: 1 },
      { user_uid: 'remote', session_uid: 'remote-session', user_name: 'Remote', audio_enabled: true, video_enabled: true, screen_sharing: false, joined_at: 2 }
    ];
    expect(callPeerIdentities(participants, 'self')).toEqual([{ userUid: 'remote', sessionUid: 'remote-session' }]);
    expect(callSessionWasReplaced(participants, 'self', 'self-session')).toBe(false);
    expect(callSessionWasReplaced(participants, 'self', 'other-browser-session')).toBe(true);
    expect(callSessionWasReplaced(participants, 'missing', 'other-browser-session')).toBe(false);
  });
});
