import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ActiveCallSummary, CallParticipant } from '../api/domain';
import { callAlertStrategy, startCallAlertCadence } from './alerts';

const caller: CallParticipant = {
  user_uid: 'caller', session_uid: 'caller-session', user_name: 'Caller', audio_enabled: true, video_enabled: false,
  screen_sharing: false, joined_at: 10
};

function call(channelKind: 'direct' | 'group', mode: 'voice' | 'video'): ActiveCallSummary {
  return { channel_uid: channelKind, channel_name: channelKind === 'direct' ? 'Caller' : 'Operations', channel_kind: channelKind, mode, started_at: Date.now(), participants: [caller] };
}

afterEach(() => vi.useRealTimers());

describe('incoming call alerts', () => {
  it('uses an urgent repeating strategy for a private call and a single chime for a group call', () => {
    expect(callAlertStrategy(call('direct', 'video'), 'Caller')).toMatchObject({
      kind: 'direct', requireInteraction: true, repeatMs: 3_000, title: 'Incoming private video call from Caller'
    });
    expect(callAlertStrategy(call('group', 'voice'), 'Caller')).toMatchObject({
      kind: 'group', requireInteraction: false, repeatMs: null, title: 'Caller started a voice call'
    });
  });

  it('repeats a private ringtone only inside its bounded ringing window and stops on cleanup', () => {
    vi.useFakeTimers();
    const play = vi.fn();
    const strategy = callAlertStrategy(call('direct', 'voice'), 'Caller');
    const stop = startCallAlertCadence(strategy, play, 21_500);
    expect(play).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(6_100);
    expect(play).toHaveBeenCalledTimes(3);
    stop();
    vi.advanceTimersByTime(30_000);
    expect(play).toHaveBeenCalledTimes(3);
  });

  it('chimes once for a fresh group call and stays silent for a stale call', () => {
    const play = vi.fn();
    const strategy = callAlertStrategy(call('group', 'voice'), 'Caller');
    startCallAlertCadence(strategy, play, 1_000);
    startCallAlertCadence(strategy, play, strategy.audibleWindowMs + 1);
    expect(play).toHaveBeenCalledTimes(1);
  });

  it('treats browser audio rejection as best effort instead of failing call notification handling', async () => {
    const play = vi.fn(async () => { throw new DOMException('Autoplay blocked', 'NotAllowedError'); });
    startCallAlertCadence(callAlertStrategy(call('group', 'voice'), 'Caller'), play);
    await Promise.resolve();
    await Promise.resolve();
    expect(play).toHaveBeenCalledOnce();
  });

  it('stops an in-progress ringtone source as soon as the alert is answered or declined', async () => {
    const stopSound = vi.fn();
    const stopAlert = startCallAlertCadence(callAlertStrategy(call('direct', 'voice'), 'Caller'), async () => stopSound);
    await Promise.resolve();
    stopAlert();
    expect(stopSound).toHaveBeenCalledOnce();
  });
});
