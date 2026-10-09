import { writable } from 'svelte/store';
import { ApiError, clearAuthTokens, hasAuthTokens } from '../api/client';
import { loadSession } from '../api/auth';
import type { Session } from '../api/types';

export const currentSession = writable<Session | null>(null);

export async function restoreSession(): Promise<Session | null> {
  if (!hasAuthTokens()) return null;

  try {
    const session = await loadSession();
    currentSession.set(session);
    return session;
  } catch (reason) {
    if (reason instanceof ApiError && [401, 403].includes(reason.status)) {
      clearAuthTokens(); currentSession.set(null); return null;
    }
    throw reason;
  }
}

export async function reloadSession(): Promise<Session> {
  const session = await loadSession();
  currentSession.set(session);
  return session;
}

export function endSession(): void {
  clearAuthTokens();
  currentSession.set(null);
}
