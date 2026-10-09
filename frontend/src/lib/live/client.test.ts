import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MxLiveClient } from './client';
import { apiJson } from '../api/client';
vi.mock('../api/client', () => ({ apiJson: vi.fn(), hasAuthTokens: () => true }));

class Socket {
  static OPEN = 1;
  static instances: Socket[] = [];
  readyState = 0;
  closed = false;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  constructor(public url: string) { Socket.instances.push(this); }
  open() { this.readyState = 1; this.onopen?.(); }
  message() { this.onmessage?.({ data: '{"type":"heartbeat"}' }); }
  close() { this.closed = true; this.readyState = 3; }
}
let live: MxLiveClient;
const ticket = { ticket: 'ticket', heartbeat_ms: 5000, stale_after_ms: 15000 };
beforeEach(() => {
  vi.useFakeTimers(); vi.setSystemTime(100000);
  Socket.instances = [];
  vi.stubGlobal('WebSocket', Socket);
  vi.stubGlobal('window', Object.assign(new EventTarget(), { setTimeout, clearTimeout, setInterval, clearInterval }));
  vi.stubGlobal('document', new EventTarget());
  vi.stubGlobal('location', { protocol: 'https:', host: 'mx.example' });
  vi.mocked(apiJson).mockReset().mockResolvedValue(ticket);
  live = new MxLiveClient(() => undefined);
});
afterEach(() => { live.stop(); vi.useRealTimers(); vi.unstubAllGlobals(); });

describe('live connection recovery', () => {
  it('detects a half-open socket that never reports close', async () => {
    live.start(); await vi.advanceTimersByTimeAsync(0);
    const old = Socket.instances[0]; old.open();
    await vi.advanceTimersByTimeAsync(21000);
    expect(old.closed).toBe(true);
    expect(Socket.instances).toHaveLength(2);
  });
  it('keeps a healthy socket alive while heartbeat messages arrive', async () => {
    live.start(); await vi.advanceTimersByTimeAsync(0);
    const socket = Socket.instances[0]; socket.open();
    for (let i = 0; i < 10; i++) { await vi.advanceTimersByTimeAsync(5000); socket.message(); }
    expect(socket.closed).toBe(false);
    expect(Socket.instances).toHaveLength(1);
  });
  it('retries a socket which never completes its handshake', async () => {
    live.start(); await vi.advanceTimersByTimeAsync(11000);
    expect(Socket.instances[0].closed).toBe(true);
    expect(Socket.instances).toHaveLength(2);
  });
  it('does not install a stale ticket after stop and restart', async () => {
    let first!: (value: unknown) => void;
    vi.mocked(apiJson).mockImplementationOnce(() => new Promise((resolve) => first = resolve));
    live.start(); live.stop(); live.start();
    await vi.advanceTimersByTimeAsync(0);
    first(ticket); await vi.advanceTimersByTimeAsync(0);
    expect(Socket.instances).toHaveLength(1);
    live.stop(); await vi.advanceTimersByTimeAsync(120000);
    expect(Socket.instances).toHaveLength(1);
  });
  it('reconnects promptly when the browser reports network recovery', async () => {
    vi.mocked(apiJson).mockRejectedValueOnce(new TypeError('offline'));
    live.start(); await vi.advanceTimersByTimeAsync(0);
    window.dispatchEvent(new Event('online'));
    await vi.advanceTimersByTimeAsync(0);
    expect(Socket.instances).toHaveLength(1);
  });
});
