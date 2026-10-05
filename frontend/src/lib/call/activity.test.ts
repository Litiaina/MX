import { describe, expect, it } from 'vitest';
import type { ActiveCallSummary, CallParticipant } from '../api/domain';
import { callAlertTitle, callDisplayName, callFromStartedPayload, mergeCallParticipant, removeCallParticipant } from './activity';

const caller: CallParticipant = {
  user_uid: 'caller', session_uid: 'caller-session', user_name: 'Caller', audio_enabled: true, video_enabled: false,
  screen_sharing: false, joined_at: 10
};

describe('call activity', () => {
  it('distinguishes private and group call notifications', () => {
    const direct = callFromStartedPayload({ channel_uid: 'direct', channel_name: 'Caller', channel_kind: 'direct', mode: 'voice', participant: caller }, 100)!;
    const group = callFromStartedPayload({ channel_uid: 'group', channel_name: 'Operations', channel_kind: 'group', mode: 'video', participant: caller }, 200)!;
    expect(callAlertTitle(direct, caller.user_name)).toBe('Incoming private call from Caller');
    expect(callAlertTitle(group, caller.user_name)).toBe('Caller started a group call');
    expect(group).toMatchObject({ mode: 'video', started_at: 200 });
    expect(callDisplayName(direct, 'recipient')).toBe('Caller');
    expect(callDisplayName(group, 'recipient')).toBe('Operations');
  });

  it('tracks joins, updates, and removes the call after its final participant leaves', () => {
    const call: ActiveCallSummary = {
      channel_uid: 'group', channel_name: 'Operations', channel_kind: 'group', mode: 'voice', started_at: 1, participants: [caller]
    };
    const second = { ...caller, user_uid: 'second', user_name: 'Second', joined_at: 20 };
    let calls = mergeCallParticipant([call], 'group', second);
    expect(calls[0].participants.map((participant) => participant.user_uid)).toEqual(['caller', 'second']);
    calls = mergeCallParticipant(calls, 'group', { ...second, screen_sharing: true });
    expect(calls[0].participants.find((participant) => participant.user_uid === 'second')?.screen_sharing).toBe(true);
    calls = removeCallParticipant(calls, 'group', 'caller');
    expect(calls[0].participants).toHaveLength(1);
    const rejoined = mergeCallParticipant(calls, 'group', { ...second, session_uid: 'new-session', joined_at: 30 });
    expect(removeCallParticipant(rejoined, 'group', 'second', 'old-session')[0].participants).toHaveLength(1);
    expect(removeCallParticipant(calls, 'group', 'second')).toEqual([]);
  });
});
