/** Shared, FIFO part budget. More throughput for one large file, not more
 * aggregate buffering when several upload jobs compete in one document. */
export class UploadSlots {
  private active = 0;
  private pending: (() => void)[] = [];
  constructor(readonly limit = 8) {}
  acquire(signal?: AbortSignal): Promise<() => void> {
    return new Promise((resolve, reject) => {
      const cancel = () => {
        this.pending = this.pending.filter(entry => entry !== enter);
        reject(signal?.reason || new DOMException('Cancelled', 'AbortError'));
      };
      const enter = () => {
        signal?.removeEventListener('abort', cancel);
        this.active++;
        let released = false;
        resolve(() => {
          if (released) return;
          released = true; this.active--;
          this.pending.shift()?.();
        });
      };
      if (signal?.aborted) { cancel(); return; }
      if (this.active < this.limit) enter();
      else { this.pending.push(enter); signal?.addEventListener('abort', cancel, { once: true }); }
    });
  }
}
export const driveUploadSlots = new UploadSlots(8);
