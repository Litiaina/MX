/** Console-style bounded, smoothed transfer telemetry. Resumed bytes are not speed. */
export class UploadMeter {
  private inflight = new Map<number, number>();
  private wireBytes = 0;
  private sampledBytes = 0;
  private sampledAt: number;
  private speed = 0;
  private lastMovement: number;
  constructor(readonly total: number, private committed = 0, private now = () => performance.now()) { this.sampledAt = this.lastMovement = now(); }
  part(index: number, loaded: number, size: number) {
    const next = Math.max(0, Math.min(size, loaded));
    this.wireBytes += Math.max(0, next - (this.inflight.get(index) || 0));
    if (next > (this.inflight.get(index) || 0)) this.lastMovement = this.now();
    this.inflight.set(index, next);
  }
  stored(index: number, size: number) {
    // Some browsers emit only a final progress event, or none for an empty part.
    this.part(index, size, size);
    this.inflight.delete(index); this.committed += size;
  }
  snapshot() {
    const now = this.now(); const elapsed = (now - this.sampledAt) / 1000;
    if (elapsed >= .2) {
      const instant = (this.wireBytes - this.sampledBytes) / elapsed;
      this.speed = this.speed ? .8 * this.speed + .2 * instant : instant;
      if (now - this.lastMovement >= 5000) this.speed = 0;
      this.sampledAt = now; this.sampledBytes = this.wireBytes;
    }
    const loaded = Math.min(this.total, this.committed + [...this.inflight.values()].reduce((sum, value) => sum + value, 0));
    return { loaded, bytesPerSecond: this.speed, etaSeconds: this.speed > 0 ? Math.ceil(Math.max(0, this.total - loaded) / this.speed) : null };
  }
}

export function uploadEta(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds)) return 'Estimating time…';
  if (seconds <= 0) return 'Waiting for storage acknowledgement…';
  const value = Math.ceil(seconds);
  if (value < 60) return `${value}s remaining`;
  if (value < 3600) return `${Math.floor(value / 60)}m ${value % 60}s remaining`;
  return `${Math.floor(value / 3600)}h ${Math.floor(value % 3600 / 60)}m remaining`;
}
