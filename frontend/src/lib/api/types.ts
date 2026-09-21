export interface AuthResponse {
  access_token: string;
  refresh_token: string;
  token_type: string;
  expires_in: number;
  uid: string;
  email: string;
  access_level: number;
  access_name: string;
}

export interface Session {
  uid: string;
  email: string;
  name: string;
  access_level: number;
  access_name: string;
  totp_enabled: boolean;
  totp_enrollment_pending: boolean;
  recovery_codes_remaining: number;
}

export interface OperationResponse {
  response: string;
  session_invalidated?: boolean;
}

export interface RecoveryCodesResponse extends OperationResponse {
  recovery_codes: string[];
  session_invalidated: boolean;
}

export interface TotpEnrollmentResponse {
  secret: string;
  otpauth_uri: string;
  qr_code_data_url: string;
  expires_in: number;
}

export interface SecondFactor {
  otp: string | null;
  recovery_code: string | null;
}
