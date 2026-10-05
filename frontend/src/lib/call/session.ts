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

function sameParticipant(left: CallParticipant, right: CallParticipant): boolean {
  return left.user_uid === right.user_uid
    && left.session_uid === right.session_uid
    && left.user_name === right.user_name
    && left.profile_photo_updated_at === right.profile_photo_updated_at
    && left.audio_enabled === right.audio_enabled
    && left.video_enabled === right.video_enabled
    && left.screen_sharing === right.screen_sharing
    && left.joined_at === right.joined_at;
}

/** Preserve participant identity when a heartbeat contains no actual change. */
export function reconcileCallParticipants(current: CallParticipant[], incoming: CallParticipant[]): CallParticipant[] {
  const existing = new Map(current.map((participant) => [
    `${participant.user_uid}:${participant.session_uid}`,
    participant
  ]));
  const reconciled = incoming.map((participant) => {
    const previous = existing.get(`${participant.user_uid}:${participant.session_uid}`);
    return previous && sameParticipant(previous, participant) ? previous : participant;
  });

  return reconciled.length === current.length
    && reconciled.every((participant, index) => participant === current[index])
    ? current
    : reconciled;
}
