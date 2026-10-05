import type { ActiveCallSummary } from '../api/domain';

export type CallAlertKind = 'direct' | 'group';

export interface CallAlertStrategy {
  kind: CallAlertKind;
  title: string;
  body: string;
  requireInteraction: boolean;
  repeatMs: number | null;
  audibleWindowMs: number;
}

export function callAlertStrategy(call: ActiveCallSummary, callerName: string): CallAlertStrategy {
  const video = call.mode === 'video';
  if (call.channel_kind === 'direct') {
    return {
      kind: 'direct',
      title: `Incoming private ${video ? 'video' : 'voice'} call from ${callerName}`,
      body: 'Open MX to answer or decline.',
      requireInteraction: true,
      repeatMs: 3_000,
      audibleWindowMs: 30_000
    };
  }
  return {
    kind: 'group',
    title: `${callerName} started a ${video ? 'video' : 'voice'} call`,
    body: `${call.channel_name} · Join when you are ready.`,
    requireInteraction: false,
    repeatMs: null,
    audibleWindowMs: 12_000
  };
}

/**
 * Plays once for group calls and repeats for a bounded period for direct calls.
 * The returned cleanup is idempotent so every answer/dismiss/end path can call it.
 */
export function startCallAlertCadence(
  strategy: CallAlertStrategy,
  play: () => void | (() => void) | Promise<void | (() => void)>,
  elapsedMs = 0
): () => void {
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const activeSounds = new Set<() => void>();
  const remainingMs = strategy.audibleWindowMs - Math.max(0, elapsedMs);
  if (remainingMs <= 0) return () => undefined;

  const safelyPlay = () => {
    try {
      void Promise.resolve(play()).then((stopSound) => {
        if (typeof stopSound !== 'function') return;
        if (stopped) stopSound();
        else activeSounds.add(stopSound);
      }).catch(() => undefined);
    }
    catch { /* Audio is best effort. */ }
  };
  const stop = () => {
    if (stopped) return;
    stopped = true;
    if (timer !== undefined) clearTimeout(timer);
    for (const stopSound of activeSounds) stopSound();
    activeSounds.clear();
  };
  safelyPlay();
  if (strategy.repeatMs === null) return stop;

  const deadline = Date.now() + remainingMs;
  const schedule = () => {
    const delay = Math.min(strategy.repeatMs!, Math.max(0, deadline - Date.now()));
    if (stopped || delay <= 0) return;
    timer = setTimeout(() => {
      if (stopped || Date.now() >= deadline) return;
      safelyPlay();
      schedule();
    }, delay);
  };
  schedule();
  return stop;
}
