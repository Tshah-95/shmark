use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    Ok { ok: serde_json::Value },
    Err { err: ApiError },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl Response {
    pub fn ok(value: serde_json::Value) -> Self {
        Response::Ok { ok: value }
    }

    pub fn err(code: &str, message: impl Into<String>) -> Self {
        Response::Err {
            err: ApiError {
                code: code.to_string(),
                message: message.into(),
            },
        }
    }
}
