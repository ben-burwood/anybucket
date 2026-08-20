//! HTTP adapter for core's [`AppError`].
//!
//! The orphan rule prevents implementing `IntoResponse` directly on the foreign `AppError`, so we newtype-wrap it.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use anybucket_core::error::AppError;

/// Wrapper so `AppError` can be returned from axum handlers as a JSON response.
pub struct ApiError(pub AppError);

impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        ApiError(err)
    }
}

impl ApiError {
    /// Map each error variant to the most appropriate HTTP status.
    fn status(&self) -> StatusCode {
        match self.0 {
            // Access-mode gate / no selection: the request is understood but refused.
            AppError::NoActiveConnection
            | AppError::ReadOnly
            | AppError::DeleteNotAllowed
            | AppError::AdminNotAllowed => StatusCode::FORBIDDEN,
            // The named/needed connection or its credentials are absent.
            AppError::ConnectionNotFound(_) | AppError::MissingCredentials(_) => {
                StatusCode::NOT_FOUND
            }
            // Malformed configuration / input.
            AppError::Config(_) => StatusCode::BAD_REQUEST,
            // Provider can't do it.
            AppError::Unsupported(_) => StatusCode::NOT_IMPLEMENTED,
            // Upstream S3 / transfer failures — a bad gateway from the browser's view.
            AppError::S3(_)
            | AppError::Download(_)
            | AppError::Upload(_)
            | AppError::Delete(_)
            | AppError::Copy(_) => StatusCode::BAD_GATEWAY,
            // Local/secret/other faults are our fault.
            AppError::Secret(_) | AppError::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Body = core's `{kind, message}` JSON, unchanged from the Tauri contract.
        (self.status(), Json(self.0)).into_response()
    }
}
