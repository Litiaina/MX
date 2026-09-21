import { apiJson, jsonRequest } from './client';
import type {
  OperationResponse,
  RecoveryCodesResponse,
  SecondFactor,
  TotpEnrollmentResponse
} from './types';

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
