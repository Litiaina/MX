import type { AuthResponse } from './types';

const ACCESS_TOKEN_KEY = 'mx_access_token';
const REFRESH_TOKEN_KEY = 'mx_refresh_token';
export const AUTH_EXPIRED_EVENT = 'mx:auth-expired';

let refreshPromise: Promise<string> | null = null;
// Keep the live document's session independent of browser storage processes.
// WebKit can lose sessionStorage when its network process recovers. Explicit
// sign-out/revocation replaces this cache with empty tokens, never rehydrates it.
let runtimeTokens: Pick<AuthResponse, 'access_token' | 'refresh_token'> | null = null;

export interface ApiRequestOptions extends RequestInit {
  timeoutMs?: number;
  maxAttempts?: number;
  /** Only opt in for operations whose server contract permits replay. */
  retrySafe?: boolean;
}

export class ConnectionError extends Error {
  constructor(message: string, public readonly uncertainWrite = false) {
    super(message);
    this.name = 'ConnectionError';
  }
}

const TRANSIENT_STATUS = new Set([408, 429, 500, 502, 503, 504]);

function pause(ms: number, signal?: AbortSignal | null): Promise<void> {
  return new Promise((resolve, reject) => {
    const abort = () => { clearTimeout(timer); reject(signal?.reason || new DOMException('Cancelled', 'AbortError')); };
    const timer = setTimeout(() => { signal?.removeEventListener('abort', abort); resolve(); }, ms);
    if (signal?.aborted) abort();
    else signal?.addEventListener('abort', abort, { once: true });
  });
}

async function request<T>(path: string, options: ApiRequestOptions, consume: (response: Response) => Promise<T>, authenticated: boolean): Promise<T> {
  const { timeoutMs, maxAttempts, retrySafe, ...init } = options;
  const method = (init.method || 'GET').toUpperCase();
  const readOnly = ['GET', 'HEAD'].includes(method);
  const safe = readOnly || retrySafe === true;
  const attempts = Math.max(1, Math.min(3, safe ? maxAttempts ?? 3 : 1));
  for (let attempt = 0; ; attempt++) {
    const controller = new AbortController();
    const abort = () => controller.abort(init.signal?.reason);
    if (init.signal?.aborted) abort();
    else init.signal?.addEventListener('abort', abort, { once: true });
    const deadline = setTimeout(() => controller.abort(new DOMException('MX request timed out', 'TimeoutError')), timeoutMs ?? (safe ? 15_000 : 30_000));
    let retryDelay = 250 * 2 ** attempt + Math.random() * 250;
    try {
      const headers = new Headers(init.headers);
      if (authenticated && accessToken()) headers.set('Authorization', `Bearer ${accessToken()}`);
      const response = await fetch(path, { ...init, headers, signal: controller.signal });
      if (safe && TRANSIENT_STATUS.has(response.status) && attempt + 1 < attempts) {
        const seconds = Number(response.headers.get('Retry-After'));
        if (Number.isFinite(seconds) && seconds > 0) retryDelay = Math.min(5000, seconds * 1000);
        await response.body?.cancel();
      } else {
        return await consume(response);
      }
    } catch (reason) {
      if (init.signal?.aborted) throw init.signal.reason || new DOMException('Cancelled', 'AbortError');
      if (reason instanceof ApiError) throw reason;
      if (!safe || attempt + 1 >= attempts) {
        throw new ConnectionError(
          readOnly ? 'MX could not be reached. Check your connection and try again.'
            : 'MX did not acknowledge this operation. Your entries are retained; check its status before retrying.',
          !readOnly
        );
      }
    } finally {
      clearTimeout(deadline);
      init.signal?.removeEventListener('abort', abort);
    }
    await pause(retryDelay, init.signal);
  }
}

export class ApiError extends Error {
  constructor(message: string, public readonly status: number, public readonly payload: unknown) {
    super(message);
    this.name = 'ApiError';
  }
}

function readPayloadMessage(payload: unknown, fallback: string): string {
  if (!payload || typeof payload !== 'object') return fallback;
  const body = payload as Record<string, unknown>;
  for (const key of ['response', 'error', 'message']) {
    if (typeof body[key] === 'string' && body[key]) return body[key];
  }
  return fallback;
}

async function readJson(response: Response): Promise<unknown> {
  const text = await response.text();
  if (!text) return null;
  try {
    return JSON.parse(text);
  } catch {
    if (response.ok) throw new ApiError('MX returned an invalid response. Reconnect and try again.', 502, null);
    const contentType = response.headers.get('content-type')?.toLowerCase() || '';
    const looksLikeHtml = contentType.includes('text/html') || /^\s*<(?:!doctype|html|head|body)\b/i.test(text);
    if (looksLikeHtml) {
      return {
        response: response.status === 404
          ? 'This feature is not available from the running MX server. Restart the updated server and try again.'
          : `The MX server returned an unexpected web page (${response.status}).`
      };
    }
    const compact = text.replace(/\s+/g, ' ').trim();
    return { response: compact.length > 300 ? `${compact.slice(0, 300)}…` : compact };
  }
}

function migrateLegacyToken(key: string): string {
  let current = '';
  try { current = sessionStorage.getItem(key) || ''; } catch { /* Restricted storage. */ }
  if (current) return current;

  let legacy = '';
  try { legacy = localStorage.getItem(key) || ''; } catch { /* Restricted storage. */ }
  if (!legacy) return '';

  try { sessionStorage.setItem(key, legacy); } catch { /* Keep this document usable. */ }
  try { localStorage.removeItem(key); } catch { /* Best-effort legacy migration. */ }
  return legacy;
}

function activeTokens() {
  return runtimeTokens ??= { access_token: migrateLegacyToken(ACCESS_TOKEN_KEY), refresh_token: migrateLegacyToken(REFRESH_TOKEN_KEY) };
}

export function accessToken(): string {
  return activeTokens().access_token;
}

export function refreshToken(): string {
  return activeTokens().refresh_token;
}

export function storeAuthTokens(tokens: AuthResponse): void {
  runtimeTokens = { access_token: tokens.access_token, refresh_token: tokens.refresh_token };
  for (const [key, token] of [[ACCESS_TOKEN_KEY, tokens.access_token], [REFRESH_TOKEN_KEY, tokens.refresh_token]]) {
    try { sessionStorage.setItem(key, token); } catch { /* Session remains document-scoped. */ }
    try { localStorage.removeItem(key); } catch { /* Best-effort legacy cleanup. */ }
  }
}

export function clearAuthTokens(): void {
  runtimeTokens = { access_token: '', refresh_token: '' };
  for (const key of [ACCESS_TOKEN_KEY, REFRESH_TOKEN_KEY]) {
    try { sessionStorage.removeItem(key); } catch { /* Never reactivate stale storage. */ }
    try { localStorage.removeItem(key); } catch { /* Never reactivate stale storage. */ }
  }
}

export function hasAuthTokens(): boolean {
  return Boolean(accessToken() || refreshToken());
}

async function refreshAccessToken(): Promise<string> {
  if (refreshPromise) return refreshPromise;

  const token = refreshToken();
  if (!token) throw new Error('Your session has expired.');

  refreshPromise = (async () => {
    const response = await request('/mx/v1/auth/refresh', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ refresh_token: token }),
      retrySafe: true,
      maxAttempts: 2
    }, async (response) => ({ status: response.status, ok: response.ok, payload: await readJson(response) }), false);
    const payload = response.payload;
    if (refreshToken() !== token) throw new Error('The active session changed.');

    if (!response.ok) {
      const message = readPayloadMessage(payload, 'Unable to refresh the session.');
      if ([401, 403].includes(response.status) && refreshToken() === token) {
        clearAuthTokens();
        if (typeof window !== 'undefined') {
          window.dispatchEvent(new CustomEvent(AUTH_EXPIRED_EVENT, { detail: { message } }));
        }
      }
      throw new ApiError(message, response.status, payload);
    }

    const tokens = payload as AuthResponse;
    if (!tokens?.access_token || !tokens?.refresh_token) throw new ApiError('MX returned an invalid session response. Try reconnecting.', 502, payload);
    // A delayed refresh must not resurrect a signed-out/replaced session.
    storeAuthTokens(tokens);
    return tokens.access_token;
  })();

  try {
    return await refreshPromise;
  } finally {
    refreshPromise = null;
  }
}

export async function apiFetch(
  path: string,
  options: ApiRequestOptions = {},
  retry = true
): Promise<Response> {
  const response = await request(path, options, async (response) => response, true);
  if (response.status === 401 && retry && refreshToken()) {
    await response.body?.cancel();
    await refreshAccessToken();
    return apiFetch(path, options, false);
  }

  return response;
}

export async function apiJson<T>(
  path: string,
  options: ApiRequestOptions = {},
  authenticated = true
): Promise<T> {
  let refreshed = false;
  for (;;) {
    const result = await request(path, options, async (response) => ({ status: response.status, ok: response.ok, payload: await readJson(response) }), authenticated);
    if (authenticated && result.status === 401 && !refreshed && refreshToken()) {
      await refreshAccessToken(); refreshed = true; continue;
    }
    if (!result.ok) throw new ApiError(readPayloadMessage(result.payload, `Request failed (${result.status}).`), result.status, result.payload);
    return result.payload as T;
  }
}

/** Buffered files use a per-read idle deadline; native media URL/range playback
 * continues to stream directly and is never forced through this buffer. */
export async function apiFile(path: string, options: ApiRequestOptions = {}): Promise<{ blob: Blob; headers: Headers }> {
  let refreshed = false;
  for (;;) {
    const result = await request(path, { timeoutMs: 30 * 60_000, ...options }, async (response) => {
      const reader = response.body?.getReader();
      const chunks: BlobPart[] = [];
      try {
        if (reader) for (;;) {
          let timer: ReturnType<typeof setTimeout> | undefined;
          try {
            const next = await Promise.race([
              reader.read(),
              new Promise<never>((_resolve, reject) => { timer = setTimeout(() => {
                reject(new DOMException('File transfer stopped making progress', 'TimeoutError'));
                void reader.cancel().catch(() => undefined);
              }, 60_000); })
            ]);
            if (next.done) break;
            chunks.push(next.value);
          } finally { clearTimeout(timer); }
        }
        const blob = reader ? new Blob(chunks, { type: response.headers.get('content-type') || '' }) : await response.blob();
        if (!response.ok) return { status: response.status, payload: await readJson(new Response(blob, { status: response.status, headers: response.headers })), file: null };
        return { status: response.status, payload: null, file: { blob, headers: response.headers } };
      } finally { reader?.releaseLock(); }
    }, true);
    if (result.status === 401 && !refreshed && refreshToken()) { await refreshAccessToken(); refreshed = true; continue; }
    if (!result.file) throw new ApiError(readPayloadMessage(result.payload, `File transfer failed (${result.status}).`), result.status, result.payload);
    return result.file;
  }
}

export async function apiUpload<T>(
  path: string,
  body: FormData | Blob,
  onProgress?: (loaded: number, total: number) => void,
  retry = true,
  signal?: AbortSignal,
  method: 'POST' | 'PUT' = 'POST'
): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const request = new XMLHttpRequest();
    let idleTimer: ReturnType<typeof setTimeout>;
    let expired = false;
    const cleanup = () => { clearTimeout(idleTimer); signal?.removeEventListener('abort', cancel); };
    const cancel = () => request.abort();
    const progressDeadline = (ms = 60_000) => {
      clearTimeout(idleTimer);
      idleTimer = setTimeout(() => { expired = true; request.abort(); }, ms);
    };
    request.open(method, path);
    request.timeout = 30 * 60_000;
    request.setRequestHeader('Accept', 'application/json');
    if (body instanceof Blob) request.setRequestHeader('Content-Type', 'application/octet-stream');
    const token = accessToken();
    if (token) request.setRequestHeader('Authorization', `Bearer ${token}`);
    request.upload.onprogress = (event) => { progressDeadline(); onProgress?.(event.loaded, event.lengthComputable ? event.total : 0); };
    request.upload.onload = () => progressDeadline(5 * 60_000);
    request.onprogress = () => progressDeadline(5 * 60_000);
    request.onerror = () => { cleanup(); reject(new ConnectionError('The upload connection was interrupted. Check the attachment list before retrying.', true)); };
    request.ontimeout = () => { cleanup(); reject(new ConnectionError('The upload timed out. Check the attachment list before retrying.', true)); };
    request.onabort = () => { cleanup(); reject(signal?.aborted ? signal.reason || new DOMException('Cancelled', 'AbortError') : new ConnectionError(expired ? 'The upload stopped making progress. Check the attachment list before retrying.' : 'Upload cancelled.', true)); };
    request.onload = async () => {
      cleanup();
      let payload: unknown = null;
      try { payload = request.responseText ? JSON.parse(request.responseText) : null; }
      catch {
        if (request.status >= 200 && request.status < 300) {
          reject(new ConnectionError('MX returned an invalid upload acknowledgement. Check the attachment list before retrying.', true));
          return;
        }
        const text = request.responseText || '';
        const contentType = request.getResponseHeader('content-type')?.toLowerCase() || '';
        const looksLikeHtml = contentType.includes('text/html') || /^\s*<(?:!doctype|html|head|body)\b/i.test(text);
        const compact = text.replace(/\s+/g, ' ').trim();
        payload = text ? {
          response: looksLikeHtml
            ? `The MX server returned an unexpected web page (${request.status}).`
            : compact.length > 300 ? `${compact.slice(0, 300)}…` : compact
        } : null;
      }
      if (request.status === 401 && retry && refreshToken()) {
        try { await refreshAccessToken(); onProgress?.(0, body instanceof Blob ? body.size : 0); resolve(await apiUpload<T>(path, body, onProgress, false, signal, method)); }
        catch (reason) { reject(reason); }
        return;
      }
      if (request.status < 200 || request.status >= 300) {
        reject(new ApiError(readPayloadMessage(payload, `Upload failed (${request.status}).`), request.status, payload));
        return;
      }
      resolve(payload as T);
    };
    if (signal?.aborted) { reject(signal.reason || new DOMException('Cancelled', 'AbortError')); return; }
    signal?.addEventListener('abort', cancel, { once: true });
    progressDeadline();
    request.send(body);
  });
}

/** Replay only an immutable multipart slot, never an arbitrary upload mutation. */
export async function apiUploadPart(path: string, body: Blob, onProgress?: (loaded: number, total: number) => void, signal?: AbortSignal): Promise<unknown> {
  for (let attempt = 0; ; attempt++) {
    signal?.throwIfAborted();
    try { return await apiUpload(path, body, onProgress, true, signal, 'PUT'); }
    catch (reason) {
      if (signal?.aborted) throw signal.reason || reason;
      if (attempt >= 4 || !(reason instanceof ConnectionError || reason instanceof ApiError && TRANSIENT_STATUS.has(reason.status))) throw reason;
      onProgress?.(0, body.size);
      await pause(Math.min(1000 * 2 ** attempt, 12_000) + Math.random() * 250, signal);
    }
  }
}

export function jsonRequest(method: string, body: unknown): RequestInit {
  return {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body)
  };
}
