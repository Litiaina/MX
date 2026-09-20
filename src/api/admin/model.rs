use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct User {
    pub uid: String,
    pub email: String,
    pub password_hash: String,
    pub name: String,
    pub created_at: i64,
    pub access_level: i64,
    pub totp_secret: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UserSummary {
    pub uid: String,
    pub email: String,
    pub name: String,
    pub access_level: i64,
    pub access_name: String,
    pub totp_enabled: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateUserRequest {
    pub email: String,
    pub password: String,
    pub name: String,
    pub access_level: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ModifySuperUserRequest {
    pub filter: String,
    pub value: String,
    pub new_email: Option<String>,
    pub new_password: Option<String>,
    pub new_name: Option<String>,
    pub access_level: Option<i64>,
}

#[derive(Deserialize)]
pub struct AdminPasswordResetRequest {
    pub new_password: String,
    pub admin_password: String,
    pub admin_otp: Option<String>,
    pub admin_recovery_code: Option<String>,
}

#[derive(Deserialize)]
pub struct AdminSecurityResetRequest {
    pub admin_password: String,
    pub admin_otp: Option<String>,
    pub admin_recovery_code: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FilterRequest {
    pub filter: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteQueryRequest {
    pub query: String,
}
