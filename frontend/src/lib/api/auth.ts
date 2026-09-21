import { apiJson, jsonRequest, storeAuthTokens } from './client';
import type { AuthResponse, SecondFactor, Session } from './types';

export function secondFactor(value: string): SecondFactor {
  const factor = value.trim();
  if (!factor) return { otp: null, recovery_code: null };
  return /^\d{6}$/.test(factor)
    ? { otp: factor, recovery_code: null }
    : { otp: null, recovery_code: factor };
}

export async function bootstrapRequired(): Promise<boolean> {
  const result = await apiJson<{ setup_required: boolean }>(
    '/mx/v1/auth/bootstrap/status',
    { method: 'GET' },
    false
  );
  return result.setup_required === true;
}

export async function bootstrapAdministrator(input: {
  authKey: string;
  name: string;
  email: string;
  password: string;
}): Promise<void> {
  await apiJson<{ response: string }>(
    '/mx/v1/auth/create',
    {
      ...jsonRequest('POST', {
        name: input.name,
        email: input.email,
        password: input.password
      }),
      headers: {
        Authorization: `Bearer ${input.authKey}`,
        'Content-Type': 'application/json'
      }
    },
    false
  );
}

export async function authenticate(
  email: string,
  password: string,
  factor: string
): Promise<AuthResponse> {
  const response = await apiJson<AuthResponse>(
    '/mx/v1/auth/authenticate',
    jsonRequest('POST', {
      email: email.trim(),
      password,
      ...secondFactor(factor)
    }),
    false
  );
  storeAuthTokens(response);
  return response;
}

export function loadSession(): Promise<Session> {
  return apiJson<Session>('/mx/v1/auth/session', { method: 'GET' });
}
