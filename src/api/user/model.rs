use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct AuthModifyUserRequest {
    pub email: String,
    pub password: String,
    pub otp: Option<String>,
    pub new_email: Option<String>,
    pub new_password: Option<String>,
    pub new_name: Option<String>,

    // Kept for API compatibility.
    // Self-service profile modification ignores this field.
    // Only an Administrator may change access levels through admin routes.
    pub access_level: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct NewUserData {
    pub new_email: Option<String>,
    pub new_password: Option<String>,
    pub new_name: Option<String>,
    pub access_level: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct QueryFilter {
    pub filter: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DeleteUserRequest {
    pub email: String,
    pub password: String,
    pub otp: Option<String>,
}
