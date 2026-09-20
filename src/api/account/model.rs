use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ProfileUpdateRequest {
    pub current_password: String,
    pub otp: Option<String>,
    pub recovery_code: Option<String>,
    pub name: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PasswordChangeRequest {
    pub current_password: String,
    pub otp: Option<String>,
    pub recovery_code: Option<String>,
    pub new_password: String,
}

#[derive(Debug, Deserialize)]
pub struct TotpEnrollmentRequest {
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct TotpConfirmRequest {
    pub password: String,
    pub otp: String,
}

#[derive(Debug, Deserialize)]
pub struct SecurityReauthRequest {
    pub password: String,
    pub otp: Option<String>,
    pub recovery_code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TotpEnrollmentResponse {
    pub secret: String,
    pub otpauth_uri: String,
    pub qr_code_data_url: String,
    pub expires_in: i64,
}

#[derive(Debug, Serialize)]
pub struct RecoveryCodesResponse {
    pub response: &'static str,
    pub recovery_codes: Vec<String>,
    pub session_invalidated: bool,
}
