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
  #generation = 0;
  #connecting: AbortController | null = null;
  #watchdog: number | null = null;
  #lastMessageAt = 0;
  #staleAfterMs = 65_000;
  #resume = () => {
    if (this.#stopped) return;
    if (this.#socket && Date.now() - this.#lastMessageAt > this.#staleAfterMs) this.#disconnect();
    if (!this.#socket && !this.#connecting) {
      if (this.#reconnectTimer !== null) clearTimeout(this.#reconnectTimer);
      this.#reconnectTimer = null;
      void this.#connect();
    }
  };

  constructor(
    private readonly onMessage: (message: LiveMessage) => void,
    private readonly onConnectionChanged: (connected: boolean) => void = () => undefined
  ) {}

  start(): void {
    if (!this.#stopped) return;
    this.#stopped = false;
    this.#generation += 1;
    window.addEventListener('online', this.#resume);
    document.addEventListener('visibilitychange', this.#resume);
    this.#watchdog = window.setInterval(() => {
      if (this.#socket && Date.now() - this.#lastMessageAt > this.#staleAfterMs) this.#disconnect();
    }, 5000);
    this.onConnectionChanged(false);
    void this.#connect();
  }

  stop(): void {
    this.#stopped = true;
    this.#generation += 1;
    this.#connecting?.abort(); this.#connecting = null;
    window.removeEventListener('online', this.#resume);
    document.removeEventListener('visibilitychange', this.#resume);
    if (this.#watchdog !== null) window.clearInterval(this.#watchdog);
    this.#watchdog = null;
    if (this.#reconnectTimer !== null) window.clearTimeout(this.#reconnectTimer);
    this.#reconnectTimer = null;
    if (this.#socket) {
      this.#socket.onclose = null; this.#socket.onerror = null; this.#socket.onopen = null; this.#socket.onmessage = null;
      this.#socket.close(1000, 'signed out');
    }
    this.#socket = null;
    this.onConnectionChanged(false);
  }

  async #connect(): Promise<void> {
    if (this.#stopped || this.#socket || this.#connecting) return;
    const generation = this.#generation;
    const connecting = new AbortController();
    this.#connecting = connecting;

    try {
      const ticket = await apiJson<LiveTicket>('/mx/v1/live/ticket', { method: 'POST', signal: connecting.signal, timeoutMs: 10_000 });
      if (this.#stopped || generation !== this.#generation) return;
      this.#staleAfterMs = Math.max(15_000, Math.min(120_000, Number(ticket.stale_after_ms) || 65_000));

      const scheme = location.protocol === 'https:' ? 'wss:' : 'ws:';
      const socket = new WebSocket(
        `${scheme}//${location.host}/mx/v1/live?ticket=${encodeURIComponent(ticket.ticket)}`
      );
      this.#socket = socket;
      this.#lastMessageAt = Date.now();
      const openingDeadline = window.setTimeout(() => { if (this.#socket === socket && socket.readyState !== WebSocket.OPEN) this.#disconnect(); }, 10_000);
      const current = () => !this.#stopped && generation === this.#generation && this.#socket === socket;

      socket.onopen = () => {
        if (!current()) return;
        window.clearTimeout(openingDeadline);
        this.#attempt = 0;
        this.#lastMessageAt = Date.now();
        this.onConnectionChanged(true);
      };
      socket.onmessage = (event) => {
        if (!current()) return;
        this.#lastMessageAt = Date.now();
        try {
          this.onMessage(JSON.parse(String(event.data)) as LiveMessage);
        } catch {
          // Ignore malformed notifications; REST remains authoritative.
        }
      };
      socket.onerror = () => { if (current()) this.#disconnect(); };
      socket.onclose = () => {
        window.clearTimeout(openingDeadline);
        if (!current()) return;
        this.#socket = null;
        this.onConnectionChanged(false);
        this.#scheduleReconnect();
      };
    } catch {
      if (this.#stopped || generation !== this.#generation) return;
      this.onConnectionChanged(false);
      if (!hasAuthTokens()) {
        this.stop();
        return;
      }
      this.#scheduleReconnect();
    } finally {
      if (this.#connecting === connecting) this.#connecting = null;
    }
  }

  #disconnect(): void {
    const socket = this.#socket;
    this.#socket = null;
    if (socket) {
      socket.onclose = null; socket.onerror = null; socket.onopen = null; socket.onmessage = null;
      socket.close(4000, 'connection stale');
    }
    this.onConnectionChanged(false);
    this.#scheduleReconnect();
  }

  #scheduleReconnect(): void {
    if (this.#stopped || this.#reconnectTimer !== null) return;
    this.#attempt += 1;
    const delay = Math.min(30_000, 500 * 2 ** Math.min(this.#attempt - 1, 6) * (0.75 + Math.random() * 0.5));
    this.#reconnectTimer = window.setTimeout(() => {
      this.#reconnectTimer = null;
      void this.#connect();
    }, delay);
  }
}
