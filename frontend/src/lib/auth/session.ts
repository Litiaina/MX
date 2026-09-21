import { writable } from 'svelte/store';
import { clearAuthTokens, hasAuthTokens } from '../api/client';
import { loadSession } from '../api/auth';
import type { Session } from '../api/types';

export const currentSession = writable<Session | null>(null);

export async function restoreSession(): Promise<Session | null> {
  if (!hasAuthTokens()) return null;

  try {
    const session = await loadSession();
    currentSession.set(session);
    return session;
  } catch {
    clearAuthTokens();
    currentSession.set(null);
    return null;
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
