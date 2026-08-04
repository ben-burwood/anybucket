//! Bucket-level aggregate metrics (total size, object count).
//!
//! S3 has no direct "bucket size" API, so the only provider-agnostic way to get
//! an exact size + object count is to fully paginate `ListObjectsV2` and sum.
//! That walks every key, so it is only ever run on explicit user request.

use aws_sdk_s3::Client;

use crate::error::AppResult;
use crate::models::{BucketMetrics, ScanProgress};

/// Page size for the scan.
const SCAN_MAX_KEYS: i32 = 1000;

/// Fully paginate a bucket and sum object sizes/count. `on_page` is invoked once
/// per listing page with the running (count, bytes) so the caller can stream
/// progress; the op layer itself stays free of IPC concerns.
pub async fn scan_metrics<F>(
    client: &Client,
    bucket: &str,
    mut on_page: F,
) -> AppResult<BucketMetrics>
where
    F: FnMut(i64, i64),
{
    let mut object_count: i64 = 0;
    let mut total_bytes: i64 = 0;
    let mut token: Option<String> = None;

    loop {
        let mut req = client
            .list_objects_v2()
            .bucket(bucket)
            .max_keys(SCAN_MAX_KEYS);
        if let Some(t) = &token {
            req = req.continuation_token(t);
        }

        let out = req.send().await?;

        for obj in out.contents() {
            object_count += 1;
            total_bytes += obj.size().unwrap_or(0);
        }

        on_page(object_count, total_bytes);

        match out.next_continuation_token() {
            Some(t) => token = Some(t.to_string()),
            None => break,
        }
    }

    Ok(BucketMetrics {
        total_bytes,
        object_count,
    })
}

/// Build a terminal `ScanProgress` for a completed/failed scan, so the command
/// layer has one place to construct the final event.
pub fn scan_done(object_count: i64, total_bytes: i64, error: Option<String>) -> ScanProgress {
    ScanProgress {
        object_count,
        total_bytes,
        done: true,
        error,
    }
}
