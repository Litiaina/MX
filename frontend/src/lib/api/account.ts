import { apiFetch, apiJson, apiUpload, jsonRequest } from './client';
import type {
  OperationResponse,
  RecoveryCodesResponse,
  SecondFactor,
  TotpEnrollmentResponse
} from './types';
import type { ProfilePhotoInfo } from './types';

export function uploadProfilePhoto(file: File, onProgress?: (loaded: number, total: number) => void): Promise<ProfilePhotoInfo> {
  const body = new FormData();
  body.append('photo', file);
  return apiUpload('/mx/v1/account/profile-photo', body, onProgress);
}

export function deleteProfilePhoto(): Promise<OperationResponse> {
  return apiJson('/mx/v1/account/profile-photo', { method: 'DELETE' });
}

export async function loadProfilePhoto(userUid: string, version: number): Promise<Blob> {
  const response = await apiFetch(`/mx/v1/account/profile-photo/${encodeURIComponent(userUid)}?v=${encodeURIComponent(String(version))}`);
  if (!response.ok) throw new Error('Profile photo could not be loaded.');
  return response.blob();
}

export function updateProfile(input: {
  currentPassword: string;
  factor: SecondFactor;
  name: string | null;
  email: string | null;
}): Promise<OperationResponse> {
  return apiJson(
    '/mx/v1/account/profile',
    jsonRequest('PATCH', {
      current_password: input.currentPassword,
      ...input.factor,
      name: input.name,
      email: input.email
    })
  );
}

export function changePassword(input: {
  currentPassword: string;
  newPassword: string;
  factor: SecondFactor;
}): Promise<OperationResponse> {
  return apiJson(
    '/mx/v1/account/password',
    jsonRequest('POST', {
      current_password: input.currentPassword,
      new_password: input.newPassword,
      ...input.factor
    })
  );
}

export function beginTotpEnrollment(password: string): Promise<TotpEnrollmentResponse> {
  return apiJson(
    '/mx/v1/account/totp/enroll',
    jsonRequest('POST', { password })
  );
}

export function confirmTotpEnrollment(
  password: string,
  otp: string
): Promise<RecoveryCodesResponse> {
  return apiJson(
    '/mx/v1/account/totp/confirm',
    jsonRequest('POST', { password, otp })
  );
}

export function cancelTotpEnrollment(password: string): Promise<OperationResponse> {
  return apiJson(
    '/mx/v1/account/totp/enroll',
    jsonRequest('DELETE', { password })
  );
}

export function disableTotp(
  password: string,
  factor: SecondFactor
): Promise<OperationResponse> {
  return apiJson(
    '/mx/v1/account/totp',
    jsonRequest('DELETE', { password, ...factor })
  );
}

export function regenerateRecoveryCodes(
  password: string,
  factor: SecondFactor
): Promise<RecoveryCodesResponse> {
  return apiJson(
    '/mx/v1/account/recovery-codes/regenerate',
    jsonRequest('POST', { password, ...factor })
  );
}
