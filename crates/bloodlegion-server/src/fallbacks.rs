use axum::{
    http::{Method, StatusCode, Uri},
    response::{IntoResponse, Response},
};

use crate::{IntoProblemResponse, Problem};

#[derive(Debug, Problem)]
#[problem(
    status = NOT_FOUND,
    title = "Not Found",
    detail = format!("The resource at '{}' could not be found.", self.uri),
    instance = self.uri.clone()
)]
pub struct NotFound {
    uri: String,
}

impl NotFound {
    pub fn new(uri: impl ToString) -> Self {
        Self {
            uri: uri.to_string(),
        }
    }
}

#[derive(Debug, Problem)]
#[problem(
    status = METHOD_NOT_ALLOWED,
    title = "Method Not Allowed",
    detail = format!("The HTTP method '{}' is not supported for the path '{}'.", self.method, self.uri),
    instance = self.uri.clone()
)]
pub struct MethodNotAllowed {
    pub method: String,
    pub uri: String,
}

impl MethodNotAllowed {
    pub fn new(method: impl ToString, uri: impl ToString) -> Self {
        Self {
            method: method.to_string(),
            uri: uri.to_string(),
        }
    }
}

fn is_api_path(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/")
}

pub async fn method_not_allowed_fallback(method: Method, uri: Uri) -> Response {
    if is_api_path(uri.path()) {
        MethodNotAllowed::new(method, uri).into_problem_response()
    } else {
        StatusCode::METHOD_NOT_ALLOWED.into_response()
    }
}

pub async fn fallback(uri: Uri) -> Response {
    if is_api_path(uri.path()) {
        NotFound::new(uri).into_problem_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}
