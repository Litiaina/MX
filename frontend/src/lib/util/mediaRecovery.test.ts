import {afterEach, beforeEach, describe, expect, it, vi} from 'vitest';
import {MediaRecovery, type MediaRecoveryState} from './mediaRecovery';
class Media extends EventTarget {
  src = 'https://mx.test/mx/v1/drive/media/version-bound-ticket';
  currentSrc = this.src;
  currentTime = 0;
  duration = 500;
  readyState = 0;
  error: {code: number} | null = null;
  volume = .4;
  muted = true;
  playbackRate = 1.5;
  load = vi.fn(() => { this.error = null; });
  play = vi.fn(async () => {});
  pause = vi.fn();
  removeAttribute = vi.fn();
  emit(name: string) { this.dispatchEvent(new Event(name)); }
}
function response(status: number, retry?: string) {
  return {status, ok: status >= 200 && status < 300, headers: new Headers(retry ? {'Retry-After': retry} : {}), body: {cancel: vi.fn(async () => {})}};
}
function setup() {
  const media = new Media(), states: MediaRecoveryState[] = [];
  const fetcher = vi.fn(async () => response(206)); vi.stubGlobal('fetch', fetcher);
  const recovery = new MediaRecovery(media as unknown as HTMLMediaElement, state => states.push(state));
  const fail = async (code = 2) => { media.error = {code}; media.emit('error'); await vi.advanceTimersByTimeAsync(0); };
  return {media, states, recovery, fetcher, fail};
}
beforeEach(() => { vi.useFakeTimers(); vi.stubGlobal('location', {href: 'https://mx.test/#drive', origin: 'https://mx.test'}); });
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });
describe('streaming preview recovery', () => {
  it('probes one byte, cancels its body and restores the position and playback settings', async () => {
    const {media, states, recovery, fetcher, fail} = setup();
    media.currentTime = 123; media.emit('timeupdate'); media.emit('playing');
    const reply = response(206); fetcher.mockResolvedValue(reply); await fail();
    expect(fetcher).toHaveBeenCalledWith(new URL(media.src), expect.objectContaining({headers: {Range: 'bytes=0-0'}, cache: 'no-store'}));
    expect(reply.body.cancel).toHaveBeenCalledOnce();
    await vi.advanceTimersByTimeAsync(1000); expect(media.load).toHaveBeenCalledOnce();
    media.currentTime = 0; media.emit('loadedmetadata'); expect(media.currentTime).toBe(123);
    media.emit('canplay'); expect(media.play).toHaveBeenCalledOnce();
    expect([media.volume, media.muted, media.playbackRate]).toEqual([.4, true, 1.5]);
    expect(states.at(-1)?.phase).toBe('idle'); recovery.dispose();
  });
  it.each([401,403,404,410,416])('stops rather than looping or minting new access tickets after HTTP %i', async status => {
    const {media, states, recovery, fetcher, fail} = setup(); fetcher.mockResolvedValue(response(status)); await fail();
    expect(states.at(-1)).toMatchObject({phase: 'failed', retryable: false});
    await vi.advanceTimersByTimeAsync(120000); expect(media.load).not.toHaveBeenCalled(); expect(fetcher).toHaveBeenCalledOnce(); recovery.dispose();
  });
  it.each([429,503])('honors upstream Retry-After on HTTP %i without repeated probes', async status => {
    const {media, states, recovery, fetcher, fail} = setup(); fetcher.mockResolvedValue(response(status,'25')); await fail();
    expect(states.at(-1)?.message).toContain('25s');
    await vi.advanceTimersByTimeAsync(24999); expect(media.load).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1); expect(media.load).toHaveBeenCalledOnce(); expect(fetcher).toHaveBeenCalledOnce(); recovery.dispose();
  });
  it('supports HTTP-date Retry-After and does not retry sooner than requested', async () => {
    vi.setSystemTime(new Date('2026-10-15T01:00:00Z'));
    const {states, recovery, fetcher, fail} = setup(); fetcher.mockResolvedValue(response(429, 'Thu, 15 Oct 2026 01:00:30 GMT')); await fail();
    expect(states.at(-1)?.message).toContain('30s'); recovery.dispose();
  });
  it('offers manual recovery instead of leaving a long automatic timer running', async () => {
    const {states, recovery, fetcher, fail, media} = setup(); fetcher.mockResolvedValue(response(429, '3600')); await fail();
    expect(states.at(-1)).toMatchObject({phase: 'failed', retryable: true});
    await vi.advanceTimersByTimeAsync(600000); expect(media.load).not.toHaveBeenCalled(); recovery.dispose();
  });
  it('bounds automatic attempts even when metadata briefly becomes playable', async () => {
    const {media, states, recovery, fetcher, fail} = setup();
    for (let attempt = 0; attempt < 3; attempt++) { await fail(); await vi.advanceTimersByTimeAsync(1000 * 2 ** attempt); media.emit('canplay'); }
    await fail(); expect(states.at(-1)).toMatchObject({phase: 'failed', retryable: true});
    await vi.advanceTimersByTimeAsync(60000); expect(media.load).toHaveBeenCalledTimes(3); expect(fetcher).toHaveBeenCalledTimes(4); recovery.dispose();
  });
  it('does not autoplay a video that was paused before the connection failed', async () => {
    const {media, recovery, fail} = setup(); media.emit('playing'); media.emit('pause'); await fail();
    await vi.advanceTimersByTimeAsync(1000); media.emit('canplay'); expect(media.play).not.toHaveBeenCalled(); recovery.dispose();
  });
  it('retries a reachable source-not-supported error once, then explains codec incompatibility', async () => {
    const {media, states, recovery, fail} = setup(); await fail(4); await vi.advanceTimersByTimeAsync(1000);
    await fail(4); expect(states.at(-1)?.message).toContain('codec'); expect(states.at(-1)?.retryable).toBe(false);
    expect(media.load).toHaveBeenCalledOnce(); recovery.dispose();
  });
  it('never retries corrupt/undecodable data automatically', async () => {
    const {states, recovery, fetcher, fail} = setup(); await fail(3);
    expect(states.at(-1)?.message).toContain('decode'); expect(fetcher).not.toHaveBeenCalled(); recovery.dispose();
  });
  it('allows manual retry after the bounded recovery budget is exhausted', async () => {
    const {media, recovery, fail} = setup();
    for (let attempt = 0; attempt < 4; attempt++) { await fail(); if (attempt < 3) await vi.advanceTimersByTimeAsync(1000 * 2 ** attempt); }
    recovery.retry(); expect(media.load).toHaveBeenCalledTimes(4); recovery.dispose();
  });
  it('recovers a persistent native stall, not only a terminal media error', async () => {
    const {states, recovery, fetcher, media} = setup(); media.emit('playing'); media.emit('waiting');
    await vi.advanceTimersByTimeAsync(20000); expect(fetcher).toHaveBeenCalledOnce(); expect(states.at(-1)?.phase).toBe('waiting'); recovery.dispose();
  });
  it('never resurrects playback after closing or switching modules', async () => {
    const {media, recovery, fetcher, fail} = setup(); await fail(); const count = fetcher.mock.calls.length;
    recovery.dispose(); expect(media.pause).toHaveBeenCalledOnce(); expect(media.removeAttribute).toHaveBeenCalledWith('src');
    const loads = media.load.mock.calls.length; await vi.advanceTimersByTimeAsync(120000); media.emit('canplay'); media.emit('error');
    expect(media.play).not.toHaveBeenCalled(); expect(media.load).toHaveBeenCalledTimes(loads); expect(fetcher).toHaveBeenCalledTimes(count);
  });
  it('aborts an in-flight probe on teardown and ignores its late completion', async () => {
    const {media, states, recovery, fetcher} = setup();
    let complete!: (value: ReturnType<typeof response>) => void;
    fetcher.mockImplementation(() => new Promise(resolve => complete = resolve));
    media.error = {code: 2}; media.emit('error');
    const signal = (fetcher.mock.calls[0] as unknown as [URL, RequestInit])[1].signal!;
    recovery.dispose(); expect(signal.aborted).toBe(true); const count = states.length;
    complete(response(206)); await vi.advanceTimersByTimeAsync(0); expect(states).toHaveLength(count);
  });
  it('does not interrupt playback that recovers while a connection probe is in flight', async () => {
    const {media, states, recovery, fetcher} = setup();
    let complete!: (value: ReturnType<typeof response>) => void;
    fetcher.mockImplementation(() => new Promise(resolve => complete = resolve));
    media.error = {code: 2}; media.emit('error'); media.error = null; media.emit('canplay');
    complete(response(503)); await vi.advanceTimersByTimeAsync(60000);
    expect(states.at(-1)?.phase).toBe('idle'); expect(media.load).not.toHaveBeenCalled(); recovery.dispose();
  });
  it('does not restart automatic checks after a terminal access failure', async () => {
    const {media, states, recovery, fetcher, fail} = setup();media.emit('playing');fetcher.mockResolvedValue(response(403));await fail();
    media.emit('waiting');media.emit('error');await vi.advanceTimersByTimeAsync(60000);
    expect(fetcher).toHaveBeenCalledOnce();expect(states.at(-1)).toMatchObject({phase:'failed',retryable:false});recovery.dispose();
  });
});
