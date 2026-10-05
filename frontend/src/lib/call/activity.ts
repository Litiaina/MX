import type { ActiveCallSummary, CallParticipant } from '../api/domain';

export function callFromStartedPayload(payload: Record<string, unknown>, eventAt = Date.now()): ActiveCallSummary | null {
  if (typeof payload.channel_uid !== 'string' || !payload.channel_uid || !payload.participant || typeof payload.participant !== 'object') return null;
  return {
    channel_uid: payload.channel_uid,
    channel_name: String(payload.channel_name || 'Conversation'),
    channel_kind: String(payload.channel_kind || 'group'),
    mode: payload.mode === 'video' ? 'video' : 'voice',
    started_at: Number(payload.started_at || eventAt),
    participants: [payload.participant as CallParticipant]
  };
}

export function mergeCallParticipant(calls: ActiveCallSummary[], channelUid: string, participant: CallParticipant): ActiveCallSummary[] {
  return calls.map((call) => call.channel_uid === channelUid ? {
    ...call,
    participants: [...call.participants.filter((item) => item.user_uid !== participant.user_uid), participant]
      .sort((left, right) => left.joined_at - right.joined_at)
  } : call);
}

export function removeCallParticipant(calls: ActiveCallSummary[], channelUid: string, userUid: string, sessionUid = ''): ActiveCallSummary[] {
  return calls.flatMap((call) => {
    if (call.channel_uid !== channelUid) return [call];
    const participants = call.participants.filter((participant) => (
      participant.user_uid !== userUid || (sessionUid !== '' && participant.session_uid !== sessionUid)
    ));
    return participants.length ? [{ ...call, participants }] : [];
  });
}

export function callAlertTitle(call: ActiveCallSummary, callerName: string): string {
  return call.channel_kind === 'direct'
    ? `Incoming private call from ${callerName}`
    : `${callerName} started a group call`;
}

export function callDisplayName(call: ActiveCallSummary, selfUid: string): string {
  if (call.channel_kind !== 'direct') return call.channel_name;
  return call.participants.find((participant) => participant.user_uid !== selfUid)?.user_name || call.channel_name;
}
