pub mod metrics;
pub mod ops;
pub mod progress_body;

use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::Client;
use url::Url;

use crate::connections::Connection;
use crate::error::{AppError, AppResult};

/// Build an S3 client for a connection.
pub async fn build_client(conn: &Connection, secret: &str) -> AppResult<Client> {
    let creds = Credentials::new(
        conn.access_key_id.clone(),
        secret.to_string(),
        None,
        None,
        "anybucket-static",
    );

    let mut loader = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(conn.region.clone()))
        .credentials_provider(creds);

    if let Some(endpoint) = &conn.endpoint_url {
        loader = loader.endpoint_url(endpoint);
    }

    let shared = loader.load().await;
    let conf = aws_sdk_s3::config::Builder::from(&shared)
        .force_path_style(conn.force_path_style)
        .build();

    Ok(Client::from_conf(conf))
}

/// The canonical `s3://bucket/key` URI.
pub fn s3_uri(bucket: &str, key: &str) -> String {
    format!("s3://{bucket}/{key}")
}

/// A browser-openable HTTPS URL for an object, honouring the connection's endpoint and addressing style.
/// When `version_id` is set, a `?versionId=` query is appended.
pub fn https_url(
    conn: &Connection,
    bucket: &str,
    key: &str,
    version_id: Option<&str>,
) -> AppResult<String> {
    let endpoint = conn
        .endpoint_url
        .clone()
        .unwrap_or_else(|| format!("https://s3.{}.amazonaws.com", conn.region));

    let url = Url::parse(&endpoint).map_err(|e| AppError::Config(e.to_string()))?;
    let scheme = url.scheme();
    let host = url
        .host_str()
        .ok_or_else(|| AppError::Config("endpoint has no host".into()))?;
    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
    let encoded_key = encode_key(key);

    let mut out = if conn.force_path_style {
        format!("{scheme}://{host}{port}/{bucket}/{encoded_key}")
    } else {
        format!("{scheme}://{bucket}.{host}{port}/{encoded_key}")
    };
    if let Some(v) = version_id {
        out.push_str("?versionId=");
        out.push_str(&encode_key(v));
    }
    Ok(out)
}

/// Percent-encode a key for use in a URL path, preserving `/` separators.
fn encode_key(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for b in key.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
