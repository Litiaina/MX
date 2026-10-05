import type { CallParticipant } from '../api/domain';
import type { LiveMessage } from '../live/client';
import type { CallPeerIdentity } from './controller';
import { createEphemeralSessionUid } from '../util/sessionUid';

export function createCallSessionUid(): string {
  return createEphemeralSessionUid();
}

export function callEventKey(event: LiveMessage): number | string {
  return event.sequence ?? `${event.type}:${event.at}:${JSON.stringify(event.payload)}`;
}

export function historicalCallEventKeys(events: LiveMessage[]): Set<number | string> {
  return new Set(events.map(callEventKey));
}

export function callLeaveMatches(currentSessionUid: string, eventSessionUid: string): boolean {
  return eventSessionUid === '' || eventSessionUid === currentSessionUid;
}

export function callSessionWasReplaced(participants: CallParticipant[], selfUid: string, sessionUid: string): boolean {
  const current = participants.find((participant) => participant.user_uid === selfUid);
  return Boolean(current && current.session_uid !== sessionUid);
}

export function callPeerIdentities(participants: CallParticipant[], selfUid: string): CallPeerIdentity[] {
  return participants
    .filter((participant) => participant.user_uid !== selfUid)
    .map((participant) => ({ userUid: participant.user_uid, sessionUid: participant.session_uid }));
}
