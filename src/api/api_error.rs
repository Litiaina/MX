use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug)]
pub enum SqliteError {
    SqliteDatabaseError,
    JoinError,
    NotFound,
    InvalidFilter,
    Conflict
}

#[derive(Debug, Deserialize, Serialize)]
pub enum SqliteQueryError {
    Result(Value),
    Error(Value),
    BadRequest(String),
    InternalError,
}

#[derive(Debug)]
pub enum AuthError {
    Ok,
    Unauthorized,
    Error,
    MissingTotp
}
