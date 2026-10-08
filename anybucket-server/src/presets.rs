//! Declarative connection presets loaded from a TOML config file.
//!
//! When an `anybucket.toml` is present (see [`crate::config`]), the server runs in **config mode**:
//! connections are defined here, held in memory, and never written to `connections.json`/`secrets.json`.
//! This module parses that file into core [`Connection`]s plus an id→secret map, which
//! [`anybucket_core::connections::ConnectionStore::from_presets`] consumes.
//!
//! Each `[[connections]]` entry may supply its secret inline (`secret_access_key`) or by referencing
//! an environment variable (`secret_access_key_env`); the env reference wins when both are present.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use anybucket_core::connections::{AccessMode, Connection, ConnectionInput};
use anybucket_core::error::{AppError, AppResult};

/// Top-level shape of the TOML config file.
#[derive(Debug, Deserialize)]
struct PresetFile {
    #[serde(default)]
    connections: Vec<PresetConnection>,
}

/// A single `[[connections]]` entry.
#[derive(Debug, Deserialize)]
struct PresetConnection {
    name: String,
    #[serde(default)]
    endpoint_url: Option<String>,
    region: String,
    #[serde(default)]
    force_path_style: bool,
    access_key_id: String,
    /// Inline secret. Optional if `secret_access_key_env` is given.
    #[serde(default)]
    secret_access_key: Option<String>,
    /// Name of an environment variable holding the secret. Wins over the inline value.
    #[serde(default)]
    secret_access_key_env: Option<String>,
    #[serde(default)]
    mode: AccessMode,
    #[serde(default)]
    admin: bool,
    #[serde(default)]
    bucket: Option<String>,
}

/// Parsed presets: connections (with deterministic ids) and their secrets, keyed by connection id.
pub struct Presets {
    pub connections: Vec<Connection>,
    pub secrets: HashMap<String, String>,
}

/// Parse the config file at `path` into [`Presets`].
///
/// Fails fast (as [`AppError::Config`]) on unreadable/invalid TOML, an unresolved secret, or two
/// entries whose names slugify to the same id.
pub fn load(path: &Path) -> AppResult<Presets> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| AppError::Config(format!("reading config file {}: {e}", path.display())))?;
    let file: PresetFile = toml::from_str(&raw)
        .map_err(|e| AppError::Config(format!("parsing config file {}: {e}", path.display())))?;

    let mut connections = Vec::with_capacity(file.connections.len());
    let mut secrets = HashMap::new();

    for preset in file.connections {
        let secret = resolve_secret(&preset)?;
        let id = format!("preset:{}", slug(&preset.name));

        if secrets.contains_key(&id) {
            return Err(AppError::Config(format!(
                "duplicate connection id '{id}' (two presets share the name '{}')",
                preset.name
            )));
        }

        // Reuse the existing endpoint/bucket normalization by routing through `ConnectionInput`.
        let input = ConnectionInput {
            id: Some(id.clone()),
            name: preset.name,
            endpoint_url: preset.endpoint_url,
            region: preset.region,
            force_path_style: preset.force_path_style,
            access_key_id: preset.access_key_id,
            secret_access_key: secret.clone(),
            mode: preset.mode,
            admin: preset.admin,
            bucket: preset.bucket,
        };

        connections.push(input.to_connection(id.clone()));
        secrets.insert(id, secret);
    }

    Ok(Presets {
        connections,
        secrets,
    })
}

/// Resolve the secret for a preset: env-var reference first, then inline value.
fn resolve_secret(preset: &PresetConnection) -> AppResult<String> {
    if let Some(var) = preset.secret_access_key_env.as_deref() {
        let var = var.trim();
        if !var.is_empty() {
            return std::env::var(var).map_err(|_| {
                AppError::Config(format!(
                    "connection '{}' references environment variable '{var}' for its secret, but it is not set",
                    preset.name
                ))
            });
        }
    }
    match preset.secret_access_key.as_deref() {
        Some(s) if !s.is_empty() => Ok(s.to_string()),
        _ => Err(AppError::Config(format!(
            "connection '{}' has no secret: set 'secret_access_key' or 'secret_access_key_env'",
            preset.name
        ))),
    }
}

/// Lowercase, mapping runs of non-alphanumeric characters to a single `-`, trimmed of leading/trailing `-`.
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for ch in name.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_inline_and_env_secrets_with_normalization() {
        // SAFETY: test-local env var; this test owns this unique key.
        unsafe {
            std::env::set_var("ANYBUCKET_TEST_PRESET_SECRET", "env-secret");
        }
        let toml = r#"
            [[connections]]
            name = "MinIO Local"
            endpoint_url = "  http://localhost:9000  "
            region = "us-east-1"
            force_path_style = true
            access_key_id = "minioadmin"
            secret_access_key = "inline-secret"
            mode = "readWrite"
            bucket = "  my-bucket  "

            [[connections]]
            name = "AWS Prod"
            region = "eu-west-1"
            access_key_id = "AKIA..."
            secret_access_key_env = "ANYBUCKET_TEST_PRESET_SECRET"
        "#;
        let dir = std::env::temp_dir().join(format!("anybucket-presets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("anybucket.toml");
        std::fs::write(&path, toml).unwrap();

        let presets = load(&path).expect("parses");
        assert_eq!(presets.connections.len(), 2);

        let minio = &presets.connections[0];
        assert_eq!(minio.id, "preset:minio-local");
        assert_eq!(minio.endpoint_url.as_deref(), Some("http://localhost:9000")); // trimmed
        assert_eq!(minio.bucket.as_deref(), Some("my-bucket")); // trimmed
        assert_eq!(minio.mode, AccessMode::ReadWrite);
        assert_eq!(presets.secrets["preset:minio-local"], "inline-secret");

        let aws = &presets.connections[1];
        assert_eq!(aws.id, "preset:aws-prod");
        assert_eq!(aws.endpoint_url, None);
        assert_eq!(presets.secrets["preset:aws-prod"], "env-secret"); // resolved from env

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn duplicate_slug_is_an_error() {
        let toml = r#"
            [[connections]]
            name = "My Store"
            region = "us-east-1"
            access_key_id = "a"
            secret_access_key = "s"

            [[connections]]
            name = "my-store"
            region = "us-east-1"
            access_key_id = "b"
            secret_access_key = "s"
        "#;
        let dir = std::env::temp_dir().join(format!("anybucket-presets-dup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("anybucket.toml");
        std::fs::write(&path, toml).unwrap();

        assert!(load(&path).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_secret_is_an_error() {
        let toml = r#"
            [[connections]]
            name = "No Secret"
            region = "us-east-1"
            access_key_id = "a"
        "#;
        let dir = std::env::temp_dir().join(format!("anybucket-presets-nosec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("anybucket.toml");
        std::fs::write(&path, toml).unwrap();

        assert!(load(&path).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
