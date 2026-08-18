//! Shared S3 core for AnyBucket.
//!
//! This crate holds everything that is independent of the runtime shell: the S3
//! operations, models, connection store, application state, and the higher-level
//! task orchestration. It is consumed by both the Tauri desktop app (`src-tauri`)
//! and the web server (`anybucket-server`).
//!
//! The two shells differ only in transport: how a request arrives, how progress
//! is delivered, and where secrets live. Progress is abstracted as a
//! [`ProgressSink`] and secrets behind [`connections::SecretStore`], so nothing
//! here depends on Tauri or on an HTTP framework.

pub mod connections;
pub mod error;
pub mod models;
pub mod s3;
pub mod state;
pub mod tasks;

use std::sync::Arc;

/// A transport-agnostic progress reporter.
///
/// The desktop shell backs this with a `tauri::ipc::Channel`; the web server
/// backs it with an NDJSON response writer. Core code just calls it. It must be
/// `Send + Sync` because upload progress is emitted from the AWS SDK's body poll,
/// which may run on a different task than the caller.
pub type ProgressSink<P> = Arc<dyn Fn(P) + Send + Sync>;
