use std::sync::Arc;

use axum::{Json, response::IntoResponse};
use reqwest::StatusCode;
use serde_json::json;
use uuid::Uuid;

use crate::{
    api::{
        admin::model::{CreateUserRequest, User},
        api_error::{AuthError, SqliteError},
        query_handler::{
            execute_create_user, execute_delete_user, execute_modify_user, verify_authentication,
        },
        user::model::{AuthModifyUserRequest, DeleteUserRequest, NewUserData, QueryFilter},
    },
    middleware::auth::{ACCESS_EDITOR, Claims, valid_access_level},
};

pub async fn create_user(
    claims: Claims,
    Json(request): Json<CreateUserRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if !claims.can_manage_accounts() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "response": "access denied" })),
        ));
    }

    let access_level = request.access_level.unwrap_or(ACCESS_EDITOR);

    if !valid_access_level(access_level) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "response": "invalid access level; allowed values are 0, 1, 2, and 3"
            })),
        ));
    }

    if request.email.trim().is_empty()
        || request.password.is_empty()
        || request.name.trim().is_empty()
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "response": "email, password, and name are required"
            })),
        ));
    }

    let created_uid = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().timestamp();

    let new_user = User {
        uid: created_uid,
        email: request.email.trim().to_string(),
        password: request.password,
        name: request.name.trim().to_string(),
        created_at,
        access_level,
        totp_secret: None,
    };

    match execute_create_user(new_user, "function", "create_user()").await {
        Ok(()) => Ok((
            StatusCode::CREATED,
            Json(json!({
                "response": "user created"
            })),
        )),

        Err(SqliteError::Conflict) => Err((
            StatusCode::CONFLICT,
            Json(json!({
                "response": "user already exists"
            })),
        )),

        Err(SqliteError::NotFound) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "response": "user not found"
            })),
        )),

        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "internal server error"
            })),
        )),
    }
}

pub async fn modify_user(
    claims: Claims,
    Json(request): Json<AuthModifyUserRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    let uid = Arc::new(claims.uid);
    let uid_clone = Arc::clone(&uid);

    let query_filter = QueryFilter {
        filter: String::from("uid"),
        value: Arc::clone(&uid).as_ref().to_string(),
    };

    /*
    Self-service profile changes are deliberately unable to modify
    access_level, even if the caller puts a value in the JSON body.
    */
    let modified_user = NewUserData {
        new_email: request.new_email,
        new_password: request.new_password,
        new_name: request.new_name,
        access_level: None,
    };

    match verify_authentication(
        uid_clone,
        request.email,
        request.password,
        request.otp,
        "function",
        "modify_user()",
    )
    .await
    {
        AuthError::Ok => {
            match execute_modify_user(query_filter, modified_user, "function", "modify_user()")
                .await
            {
                Ok(()) => Ok((
                    StatusCode::OK,
                    Json(json!({
                        "response": "user updated"
                    })),
                )),

                Err(SqliteError::Conflict) => Err((
                    StatusCode::CONFLICT,
                    Json(json!({
                        "response": "user already exists"
                    })),
                )),

                Err(SqliteError::NotFound) => Err((
                    StatusCode::NOT_FOUND,
                    Json(json!({
                        "response": "user not found"
                    })),
                )),

                Err(_) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": "internal server error"
                    })),
                )),
            }
        }

        AuthError::Unauthorized => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "response": "unauthorized"
            })),
        )),

        AuthError::Error => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "response": "internal server error"
            })),
        )),

        AuthError::MissingTotp => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "response": "otp is required"
            })),
        )),
    }
}

pub async fn delete_user(
    claims: Claims,
    Json(request): Json<DeleteUserRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    let uid = Arc::new(claims.uid);
    let uid_clone = Arc::clone(&uid);

    match verify_authentication(
        uid_clone,
        request.email,
        request.password,
        request.otp,
        "function",
        "delete_user()",
    )
    .await
    {
        AuthError::Ok => {
            match execute_delete_user(
                Arc::clone(&uid).as_ref().to_string(),
                "function",
                "delete_user()",
            )
            .await
            {
                Ok(()) => Ok(StatusCode::NO_CONTENT),

                Err(SqliteError::NotFound) => Err((
                    StatusCode::NOT_FOUND,
                    Json(json!({
                        "response": "user not found"
                    })),
                )),

                Err(_) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": "internal server error"
                    })),
                )),
            }
        }

        AuthError::Unauthorized => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "response": "unauthorized"
            })),
        )),

        AuthError::Error => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "response": "internal server error"
            })),
        )),

        AuthError::MissingTotp => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "response": "otp is required"
            })),
        )),
    }
}
