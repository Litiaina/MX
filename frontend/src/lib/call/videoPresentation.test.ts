import { afterEach, describe, expect, it, vi } from 'vitest';
import { CallVideoPresentation, sameMediaTracks, shouldStageVideoHandoff, type VideoPresentationState } from './videoPresentation';

afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

describe('call video presentation', () => {
  const receiverTrack = { kind: 'video' } as MediaStreamTrack;
  const replacementTrack = { kind: 'video' } as MediaStreamTrack;

  it('keeps the decoder bound across heartbeats and microphone-only stream wrapper changes', () => {
    expect(shouldStageVideoHandoff(receiverTrack, 'camera', receiverTrack, 'camera')).toBe(false);
    expect(shouldStageVideoHandoff(receiverTrack, 'screen', receiverTrack, 'screen')).toBe(false);
  });

  it('stages real track replacements and camera-to-screen semantic transitions', () => {
    expect(shouldStageVideoHandoff(receiverTrack, 'camera', replacementTrack, 'camera')).toBe(true);
    expect(shouldStageVideoHandoff(receiverTrack, 'camera', receiverTrack, 'screen')).toBe(true);
    expect(shouldStageVideoHandoff(receiverTrack, 'screen', receiverTrack, 'camera')).toBe(true);
  });

  it('does not stage an absent or disabled video source', () => {
    expect(shouldStageVideoHandoff(receiverTrack, 'camera', null, 'off')).toBe(false);
  });
});

class FakeVideo extends EventTarget {
  readyState = 0;
  videoWidth = 0;
  videoHeight = 0;
  paused = true;
  muted = true;
  bindings = 0;
  private source: MediaStream | null = null;
  get srcObject() { return this.source; }
  set srcObject(source: MediaStream | null) {
    this.bindings += 1;
    this.source = source;
    this.readyState = 0;
    this.videoWidth = 0;
    this.videoHeight = 0;
  }
  play = vi.fn(async () => { this.paused = false; });
  pause = vi.fn(() => { this.paused = true; });
  requestVideoFrameCallback = vi.fn((_callback: () => void) => 7);
  cancelVideoFrameCallback = vi.fn();
  decodedFrame() {
    this.readyState = 2;
    this.videoWidth = 3840;
    this.videoHeight = 2160;
    this.dispatchEvent(new Event('loadeddata'));
  }
}

describe('single-video source lifecycle', () => {
  function setup() {
    vi.useFakeTimers();
    vi.stubGlobal('MediaStream', class {
      constructor(private tracks: MediaStreamTrack[]) {}
      getVideoTracks() { return this.tracks; }
    });
    const video = new FakeVideo();
    const drawImage = vi.fn();
    const canvas = { width: 0, height: 0, getContext: () => ({ drawImage }) };
    const states: VideoPresentationState[] = [];
    const renderer = new CallVideoPresentation(video as unknown as HTMLVideoElement, canvas as unknown as HTMLCanvasElement, (state) => states.push(state));
    const camera = { kind: 'video' } as MediaStreamTrack;
    const screen = { kind: 'video' } as MediaStreamTrack;
    return { renderer, video, canvas, drawImage, states, camera, screen };
  }

  it('retains the decoder during heartbeats and displays the first static frame immediately', async () => {
    const { renderer, video, states, camera } = setup();
    renderer.setSource(camera, 'camera');
    await Promise.resolve();
    expect(states.at(-1)).toEqual({ ready: false, snapshotVisible: false });
    video.decodedFrame();
    expect(states.at(-1)).toEqual({ ready: true, snapshotVisible: false });
    for (let heartbeat = 0; heartbeat < 20; heartbeat++) renderer.setSource(camera, 'camera');
    expect(video.bindings).toBe(1);
    expect(vi.getTimerCount()).toBe(0);
    renderer.dispose();
  });

  it('uses a bounded still frame instead of two playing videos during a delayed source change', () => {
    const { renderer, video, canvas, drawImage, states, camera, screen } = setup();
    renderer.setSource(camera, 'camera');
    video.decodedFrame();
    renderer.setSource(screen, 'screen');
    expect(canvas).toMatchObject({ width: 1280, height: 720 });
    expect(drawImage).toHaveBeenCalledWith(video, 0, 0, 1280, 720);
    expect(states.at(-1)).toEqual({ ready: false, snapshotVisible: true });
    vi.advanceTimersByTime(1500);
    expect(states.at(-1)).toEqual({ ready: false, snapshotVisible: true });
    video.decodedFrame();
    expect(states.at(-1)).toEqual({ ready: true, snapshotVisible: false });
    renderer.dispose();
  });

  it('preserves the still frame through rapid switches and ignores stale readiness callbacks', async () => {
    const { renderer, video, states, camera, screen } = setup();
    renderer.setSource(camera, 'camera');
    video.decodedFrame();
    renderer.setSource(screen, 'screen');
    await Promise.resolve();
    const staleFrame = video.requestVideoFrameCallback.mock.calls.at(-1)?.[0] as unknown as (() => void);
    renderer.setSource(camera, 'camera');
    expect(states.at(-1)).toEqual({ ready: false, snapshotVisible: true });
    if (staleFrame) staleFrame();
    expect(states.at(-1)).toEqual({ ready: false, snapshotVisible: true });
    video.decodedFrame();
    expect(states.at(-1)).toEqual({ ready: true, snapshotVisible: false });
    renderer.dispose();
    expect(video.srcObject).toBeNull();
    expect(vi.getTimerCount()).toBe(0);
  });

  it('releases local playback without stopping the shared outgoing track', () => {
    const { renderer, video, states, screen } = setup();
    const stop = vi.fn();
    screen.stop = stop;
    renderer.setSource(screen, 'screen');
    video.decodedFrame();
    renderer.setSource(null, 'screen');
    expect(video.srcObject).toBeNull();
    expect(video.paused).toBe(true);
    expect(stop).not.toHaveBeenCalled();
    expect(states.at(-1)).toEqual({ ready: false, snapshotVisible: false });
    renderer.dispose();
  });

  it('keeps the audio binding when video changes leave its audio tracks unchanged', () => {
    const mic = { kind: 'audio' } as MediaStreamTrack;
    const sharedAudio = { kind: 'audio' } as MediaStreamTrack;
    expect(sameMediaTracks([mic], [mic])).toBe(true);
    expect(sameMediaTracks([mic], [mic, sharedAudio])).toBe(false);
    expect(sameMediaTracks([mic, sharedAudio], [mic, sharedAudio])).toBe(true);
    expect(sameMediaTracks([mic, sharedAudio], [mic])).toBe(false);
  });
});
