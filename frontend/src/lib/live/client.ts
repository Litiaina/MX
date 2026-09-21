import { apiJson, hasAuthTokens } from '../api/client';

interface LiveTicket {
  ticket: string;
  heartbeat_ms: number;
  stale_after_ms: number;
}

export interface LiveMessage {
  type: string;
  sequence?: number;
  at?: number;
  actor_uid?: string | null;
  payload?: unknown;
}

export class MxLiveClient {
  #socket: WebSocket | null = null;
  #reconnectTimer: number | null = null;
  #stopped = true;
  #attempt = 0;

  constructor(
    private readonly onMessage: (message: LiveMessage) => void,
    private readonly onConnectionChanged: (connected: boolean) => void = () => undefined
  ) {}

  start(): void {
    this.#stopped = false;
    this.onConnectionChanged(false);
    void this.#connect();
  }

  stop(): void {
    this.#stopped = true;
    if (this.#reconnectTimer !== null) window.clearTimeout(this.#reconnectTimer);
    this.#reconnectTimer = null;
    this.#socket?.close(1000, 'signed out');
    this.#socket = null;
    this.onConnectionChanged(false);
  }

  async #connect(): Promise<void> {
    if (this.#stopped || this.#socket) return;

    try {
      const ticket = await apiJson<LiveTicket>('/mx/v1/live/ticket', { method: 'POST' });
      if (this.#stopped) return;

      const scheme = location.protocol === 'https:' ? 'wss:' : 'ws:';
      const socket = new WebSocket(
        `${scheme}//${location.host}/mx/v1/live?ticket=${encodeURIComponent(ticket.ticket)}`
      );
      this.#socket = socket;

      socket.onopen = () => {
        this.#attempt = 0;
        this.onConnectionChanged(true);
      };
      socket.onmessage = (event) => {
        try {
          this.onMessage(JSON.parse(String(event.data)) as LiveMessage);
        } catch {
          // Ignore malformed notifications; REST remains authoritative.
        }
      };
      socket.onerror = () => socket.close();
      socket.onclose = () => {
        if (this.#socket === socket) this.#socket = null;
        this.onConnectionChanged(false);
        this.#scheduleReconnect();
      };
    } catch {
      this.onConnectionChanged(false);
      if (!hasAuthTokens()) {
        this.stop();
        return;
      }
      this.#scheduleReconnect();
    }
  }

  #scheduleReconnect(): void {
    if (this.#stopped || this.#reconnectTimer !== null) return;
    this.#attempt += 1;
    const delay = Math.min(30_000, 500 * 2 ** Math.min(this.#attempt - 1, 6));
    this.#reconnectTimer = window.setTimeout(() => {
      this.#reconnectTimer = null;
      void this.#connect();
    }, delay);
  }
}
