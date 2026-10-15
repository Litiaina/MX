export interface MediaRecoveryState {
  phase: 'idle' | 'checking' | 'waiting' | 'loading' | 'failed';
  message: string;
  retryable: boolean;
}
const idle: MediaRecoveryState = { phase: 'idle', message: '', retryable: false };
const retryableStatuses = new Set([408, 429, 500, 502, 503, 504]);

// Native media errors hide HTTP status codes. Inspect only one byte, never
// fetch/blob-buffer a large video. Keep the same ACL-checked, version-bound URL.
export class MediaRecovery {
  private stopped = false;
  private checking = false;
  private sequence = 0;
  private timer?: ReturnType<typeof setTimeout>;
  private probe?: AbortController;
  private attempts = 0;
  private position = 0;
  private progress = 0;
  private restorePosition?: number;
  private resumePlaying = false;
  private recovering = false;
  private unsupportedRetry = false;
  private automaticStopped = false;
  private readonly listeners: [string, EventListener][] = [];
  constructor(private media: HTMLMediaElement, private changed: (state: MediaRecoveryState) => void) {
    this.listen('timeupdate', () => {
      if (!this.recovering && Number.isFinite(media.currentTime)) {
        this.progress += Math.max(0, Math.min(1, media.currentTime - this.position));
        this.position = media.currentTime;
        if (this.progress >= 5) { this.attempts = 0; this.unsupportedRetry = false; this.progress = 0; }
      }
    });
    this.listen('playing', () => { this.resumePlaying = true; this.recovered(); });
    this.listen('pause', () => { if (!media.error && (!this.recovering || media.readyState >= 2)) this.resumePlaying = false; });
    this.listen('waiting', () => {
      if (this.stopped || this.automaticStopped || this.checking || this.recovering || !this.resumePlaying) return;
      clearTimeout(this.timer); this.timer = setTimeout(() => void this.inspect(), 20000);
    });
    this.listen('loadedmetadata', () => {
      if (this.restorePosition === undefined) return;
      const position = this.restorePosition;
      this.restorePosition = undefined;
      try { media.currentTime = Number.isFinite(media.duration) ? Math.min(position, Math.max(0, media.duration - .05)) : position; }
      catch { /* Native controls still allow a manual seek if not yet seekable. */ }
    });
    this.listen('canplay', () => {
      if (!this.recovering) return;
      this.recovered();
      if (this.resumePlaying) void media.play().catch(() => {
        if (!this.stopped) this.changed({ ...idle, message: 'Connection restored. Press Play to continue.' });
      });
    });
    this.listen('error', () => void this.inspect());
  }
  private listen(type: string, listener: EventListener) {
    this.listeners.push([type, listener]); this.media.addEventListener(type, listener);
  }
  private recovered() {
    if (this.stopped) return;
    this.sequence++; this.probe?.abort(); this.probe = undefined; this.checking = false;
    clearTimeout(this.timer); this.recovering = false; this.changed(idle);
    // Only actual progress resets the recovery budget, not a metadata response.
    // Otherwise a repeatedly failing server could cause an endless reload loop.
  }
  private async inspect() {
    if (this.stopped || this.automaticStopped || this.checking) return;
    clearTimeout(this.timer);
    if (!this.recovering && Number.isFinite(this.media.currentTime) && this.media.currentTime > 0) this.position = this.media.currentTime;
    if (this.media.error?.code === 1) { this.recovering = false; this.changed(idle); return; }
    if (this.media.error?.code === 3) {
      this.fail('This browser could not decode the media. Download the original or try a browser that supports its codec.', false); return;
    }
    this.recovering = true; this.checking = true;
    const sequence = ++this.sequence;
    this.changed({ phase: 'checking', message: 'Playback interrupted. Checking the connection…', retryable: false });
    let message = 'The connection was interrupted.', retryable = true, wait = Math.min(2 ** this.attempts, 8);
    const controller = new AbortController(); this.probe = controller;
    const timeout = setTimeout(() => controller.abort(), 10000);
    try {
      const url = new URL(this.media.currentSrc || this.media.src, location.href);
      if (!['http:', 'https:'].includes(url.protocol) || url.origin !== location.origin) {
        retryable = false; message = 'This browser cannot play the file. Download the original to open it in a compatible application.';
      } else {
        const response = await fetch(url, { headers: { Range: 'bytes=0-0' }, cache: 'no-store', credentials: 'same-origin', signal: controller.signal });
        // Cancel before reading: even a server that ignores Range cannot cause
        // a whole-file download or unbounded error-body buffering here.
        await response.body?.cancel().catch(() => undefined);
        if (response.ok) {
          if (this.media.error?.code === 4 && this.unsupportedRetry) {
            retryable = false; message = 'The file is reachable, but this browser does not support its media format or codec. Download the original.';
          }
          if (this.media.error?.code === 4) this.unsupportedRetry = true;
        } else {
          retryable = retryableStatuses.has(response.status);
          if (response.status === 429) message = 'Storage is temporarily limiting requests.';
          else if (response.status === 401 || response.status === 403) message = 'You no longer have access to this file. Sign in again or ask its owner.';
          else if (response.status === 404 || response.status === 410) message = 'This preview is no longer available. Close it and reopen the file from MX.';
          else message = `Storage could not continue playback (HTTP ${response.status}).`;
          const value = response.headers.get('retry-after');
          if (value) {
            const seconds = /^\d+$/.test(value.trim()) ? Number(value) : (Date.parse(value) - Date.now()) / 1000;
            if (Number.isFinite(seconds) && seconds > 0) wait = Math.max(wait, Math.ceil(seconds));
          }
        }
      }
    } catch { /* A lost connection is recoverable; abort on teardown is ignored below. */ }
    finally { clearTimeout(timeout); if (this.probe === controller) { this.probe = undefined; this.checking = false; } }
    if (this.stopped || sequence !== this.sequence) return;
    if (!retryable || this.attempts >= 3 || wait > 60) { this.fail(`${message}${retryable ? ' Use Retry playback when the connection is available.' : ''}`, retryable); return; }
    this.attempts++;
    this.changed({ phase: 'waiting', message: `${message} Retrying in ${wait}s (${this.attempts} of 3)…`, retryable: true });
    this.timer = setTimeout(() => this.reload(), wait * 1000);
  }
  private fail(message: string, retryable: boolean) {
    this.automaticStopped = true; this.recovering = false; this.changed({ phase: 'failed', message, retryable });
  }
  private reload() {
    if (this.stopped) return;
    clearTimeout(this.timer);
    this.recovering = true; this.restorePosition = this.position;
    this.changed({ phase: 'loading', message: 'Reconnecting playback…', retryable: true });
    this.media.load();
    // Native stalled media can fail without a terminal error event.
    this.timer = setTimeout(() => void this.inspect(), 20000);
  }
  retry() {
    if (this.stopped || this.checking) return;
    this.automaticStopped = false; this.attempts = 0; this.unsupportedRetry = false; this.reload();
  }
  dispose() {
    this.stopped = true; clearTimeout(this.timer); this.probe?.abort();
    for (const [type, listener] of this.listeners) this.media.removeEventListener(type, listener);
    this.media.pause(); this.media.removeAttribute('src'); this.media.load();
  }
}
