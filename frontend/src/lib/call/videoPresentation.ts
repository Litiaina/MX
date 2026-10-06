export type CallVideoMode = 'off' | 'camera' | 'screen';

export function shouldStageVideoHandoff(
  currentTrack: MediaStreamTrack | null,
  currentMode: CallVideoMode,
  nextTrack: MediaStreamTrack | null,
  nextMode: CallVideoMode
): boolean {
  return nextTrack !== null
    && nextMode !== 'off'
    && (currentTrack !== nextTrack || currentMode !== nextMode);
}

export function sameMediaTracks(left: MediaStreamTrack[], right: MediaStreamTrack[]): boolean {
  return left.length === right.length && left.every((track, index) => track === right[index]);
}

export interface VideoPresentationState {
  ready: boolean;
  snapshotVisible: boolean;
}

/** One playing video; a still frame covers real source changes without overlap. */
export class CallVideoPresentation {
  #track: MediaStreamTrack | null = null;
  #mode: CallVideoMode = 'off';
  #version = 0;
  #snapshotAvailable = false;
  #cancelPending = () => undefined;

  constructor(
    private readonly video: HTMLVideoElement,
    private readonly snapshot: HTMLCanvasElement,
    private readonly onState: (state: VideoPresentationState) => void
  ) {}

  setSource(track: MediaStreamTrack | null, mode: CallVideoMode): void {
    if (!track || mode === 'off') {
      if (this.#track || this.video.srcObject) this.clear();
      return;
    }
    if (!shouldStageVideoHandoff(this.#track, this.#mode, track, mode)) {
      if (this.video.paused) void this.video.play().catch(() => undefined);
      return;
    }

    this.#cancelPending();
    const version = ++this.#version;
    this.#track = track;
    this.#mode = mode;
    let snapshotVisible = this.#snapshotAvailable;
    if (this.video.readyState >= 2 && this.video.videoWidth && this.video.videoHeight) {
      try {
        const scale = Math.min(1, 1280 / this.video.videoWidth, 720 / this.video.videoHeight);
        this.snapshot.width = Math.max(1, Math.round(this.video.videoWidth * scale));
        this.snapshot.height = Math.max(1, Math.round(this.video.videoHeight * scale));
        const context = this.snapshot.getContext('2d');
        if (context) {
          context.drawImage(this.video, 0, 0, this.snapshot.width, this.snapshot.height);
          snapshotVisible = true;
          this.#snapshotAvailable = true;
        }
      } catch { /* A missing frame must not prevent the next source playing. */ }
    }
    this.onState({ ready: false, snapshotVisible });
    const source = new MediaStream([track]);
    const video = this.video;
    let cancelled = false;
    let frameRequest: number | undefined;
    let fallbackTimer: ReturnType<typeof setTimeout> | undefined;

    const cleanup = () => {
      video.removeEventListener('loadeddata', checkFrame);
      video.removeEventListener('playing', checkFrame);
      clearTimeout(fallbackTimer);
      if (frameRequest !== undefined) video.cancelVideoFrameCallback?.(frameRequest);
    };
    const reveal = () => {
      if (cancelled || version !== this.#version || video.srcObject !== source || video.readyState < 2 || !video.videoWidth) return;
      cancelled = true;
      cleanup();
      this.onState({ ready: true, snapshotVisible: false });
      this.#cancelPending = () => undefined;
    };
    const checkFrame = () => {
      if (cancelled || version !== this.#version) return;
      // loadeddata is already the first decoded frame; static shares need no
      // extra capture frame or opacity animation before becoming visible.
      if (video.readyState >= 2 && video.videoWidth) { reveal(); return; }
      if (frameRequest === undefined && video.requestVideoFrameCallback) {
        frameRequest = video.requestVideoFrameCallback(() => { frameRequest = undefined; reveal(); });
      }
    };
    this.#cancelPending = () => { cancelled = true; cleanup(); };
    video.addEventListener('loadeddata', checkFrame);
    video.addEventListener('playing', checkFrame);
    video.pause();
    video.srcObject = source;
    video.muted = true;
    fallbackTimer = setTimeout(reveal, 1500);
    void video.play().then(checkFrame).catch(() => undefined);
  }

  clear(): void {
    this.#version += 1;
    this.#cancelPending();
    this.#cancelPending = () => undefined;
    this.#track = null;
    this.#mode = 'off';
    this.#snapshotAvailable = false;
    this.video.pause();
    this.video.srcObject = null;
    this.onState({ ready: false, snapshotVisible: false });
  }

  dispose(): void {
    this.clear();
    this.snapshot.width = 0;
    this.snapshot.height = 0;
  }
}
