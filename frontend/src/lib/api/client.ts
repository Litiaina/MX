import type { AuthResponse } from './types';

const ACCESS_TOKEN_KEY = 'mx_access_token';
const REFRESH_TOKEN_KEY = 'mx_refresh_token';
export const AUTH_EXPIRED_EVENT = 'mx:auth-expired';

let refreshPromise: Promise<string> | null = null;

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
    return { response: text };
  }
}

function migrateLegacyToken(key: string): string {
  const current = sessionStorage.getItem(key);
  if (current) return current;

  const legacy = localStorage.getItem(key);
  if (!legacy) return '';

  sessionStorage.setItem(key, legacy);
  localStorage.removeItem(key);
  return legacy;
}

export function accessToken(): string {
  return migrateLegacyToken(ACCESS_TOKEN_KEY);
}

export function refreshToken(): string {
  return migrateLegacyToken(REFRESH_TOKEN_KEY);
}

export function storeAuthTokens(tokens: AuthResponse): void {
  sessionStorage.setItem(ACCESS_TOKEN_KEY, tokens.access_token);
  sessionStorage.setItem(REFRESH_TOKEN_KEY, tokens.refresh_token);
  localStorage.removeItem(ACCESS_TOKEN_KEY);
  localStorage.removeItem(REFRESH_TOKEN_KEY);
}

export function clearAuthTokens(): void {
  sessionStorage.removeItem(ACCESS_TOKEN_KEY);
  sessionStorage.removeItem(REFRESH_TOKEN_KEY);
  localStorage.removeItem(ACCESS_TOKEN_KEY);
  localStorage.removeItem(REFRESH_TOKEN_KEY);
}

export function hasAuthTokens(): boolean {
  return Boolean(accessToken() || refreshToken());
}

async function refreshAccessToken(): Promise<string> {
  if (refreshPromise) return refreshPromise;

  const token = refreshToken();
  if (!token) throw new Error('Your session has expired.');

  refreshPromise = (async () => {
    const response = await fetch('/mx/v1/auth/refresh', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ refresh_token: token })
    });
    const payload = await readJson(response);

    if (!response.ok) {
      const message = readPayloadMessage(payload, 'Unable to refresh the session.');
      clearAuthTokens();
      if (typeof window !== 'undefined') {
        window.dispatchEvent(new CustomEvent(AUTH_EXPIRED_EVENT, { detail: { message } }));
      }
      throw new Error(message);
    }

    const tokens = payload as AuthResponse;
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
  options: RequestInit = {},
  retry = true
): Promise<Response> {
  const headers = new Headers(options.headers);
  const token = accessToken();
  if (token) headers.set('Authorization', `Bearer ${token}`);

  const response = await fetch(path, { ...options, headers });
  if (response.status === 401 && retry && refreshToken()) {
    await refreshAccessToken();
    return apiFetch(path, options, false);
  }

  return response;
}

export async function apiJson<T>(
  path: string,
  options: RequestInit = {},
  authenticated = true
): Promise<T> {
  const response = authenticated
    ? await apiFetch(path, options)
    : await fetch(path, options);
  const payload = await readJson(response);

  if (!response.ok) {
    throw new ApiError(readPayloadMessage(payload, `Request failed (${response.status}).`), response.status, payload);
  }

  return payload as T;
}

export async function apiUpload<T>(
  path: string,
  body: FormData,
  onProgress?: (loaded: number, total: number) => void,
  retry = true
): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const request = new XMLHttpRequest();
    request.open('POST', path);
    request.setRequestHeader('Accept', 'application/json');
    const token = accessToken();
    if (token) request.setRequestHeader('Authorization', `Bearer ${token}`);
    request.upload.onprogress = (event) => onProgress?.(event.loaded, event.lengthComputable ? event.total : 0);
    request.onerror = () => reject(new Error('The upload connection was interrupted.'));
    request.onload = async () => {
      let payload: unknown = null;
      try { payload = request.responseText ? JSON.parse(request.responseText) : null; }
      catch { payload = request.responseText ? { response: request.responseText } : null; }
      if (request.status === 401 && retry && refreshToken()) {
        try { await refreshAccessToken(); resolve(await apiUpload<T>(path, body, onProgress, false)); }
        catch (reason) { reject(reason); }
        return;
      }
      if (request.status < 200 || request.status >= 300) {
        reject(new ApiError(readPayloadMessage(payload, `Upload failed (${request.status}).`), request.status, payload));
        return;
      }
      resolve(payload as T);
    };
    request.send(body);
  });
}

export function jsonRequest(method: string, body: unknown): RequestInit {
  return {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body)
  };
}
