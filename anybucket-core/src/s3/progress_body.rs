//! Byte-level upload progress: an [`UploadReporter`] wraps each file-backed
//! [`ByteStream`] so bytes are counted as the S3 SDK sends them, emitting smooth progress
//!
//! The SDK polls the upload body lazily as it writes to the socket, so tallying each data frame in
//!  [`ProgressBody::poll_frame`] tracks bytes actually handed to the HTTP layer.
//! Progress is emitted through a [`ProgressSink`], throttled to `step` bytes so we don't flood the transport.
//!
//! [`ProgressSink`]: crate::ProgressSink
//!
//! [`ByteStream`]: aws_sdk_s3::primitives::ByteStream

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use aws_sdk_s3::primitives::ByteStream;
use aws_smithy_types::body::SdkBody;
use bytes::Bytes;
use http_body::{Body, Frame, SizeHint};
use pin_project_lite::pin_project;

use crate::models::UploadProgress;
use crate::ProgressSink;

/// Tracks one upload's absolute byte progress and emits throttled events.
/// Its [`wrap`](Self::wrap) turns any file-backed `ByteStream` into a counting one;
/// a single reporter shared across a multipart upload's parts yields one continuous count.
/// Owns all AWS-SDK body plumbing so the command layer just orchestrates.
pub struct UploadReporter {
    uploaded: Arc<AtomicU64>,
    total: u64,
    key: String,
    sink: ProgressSink<UploadProgress>,
    step: u64,
}

impl UploadReporter {
    pub fn new(total: u64, key: String, sink: ProgressSink<UploadProgress>, step: u64) -> Self {
        Self {
            uploaded: Arc::new(AtomicU64::new(0)),
            total,
            key,
            sink,
            step,
        }
    }

    /// The file's total size (the progress denominator)
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Wrap a file-backed `ByteStream` so bytes are counted as the SDK sends them. Uses `map_preserve_contents`
    /// (documented for exactly this "add progress tracking without altering bytes" use case) so the Content-Length
    /// hint — and thus request signing — is intact.
    pub fn wrap(&self, inner: ByteStream) -> ByteStream {
        let uploaded = self.uploaded.clone();
        let total = self.total;
        let key = self.key.clone();
        let sink = self.sink.clone();
        let step = self.step;
        let body = inner.into_inner().map_preserve_contents(move |b| {
            SdkBody::from_body_1_x(ProgressBody {
                inner: b,
                uploaded: uploaded.clone(),
                total,
                key: key.clone(),
                sink: sink.clone(),
                step,
            })
        });
        ByteStream::new(body)
    }
}

pin_project! {
    /// Pass-through body that tallies sent data bytes into a shared counter and emits throttled [`UploadProgress`].
    /// The `uploaded` counter is shared (`Arc`) so a multipart upload reports one continuous absolute count
    /// across all its parts.
    struct ProgressBody<B> {
        #[pin]
        inner: B,
        uploaded: Arc<AtomicU64>,
        total: u64,
        key: String,
        sink: ProgressSink<UploadProgress>,
        step: u64,
    }
}

impl<B, E> Body for ProgressBody<B>
where
    B: Body<Data = Bytes, Error = E>,
{
    type Data = Bytes;
    type Error = E;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.project();
        match this.inner.poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    let len = data.len() as u64;
                    if len > 0 {
                        // Absolute bytes sent so far across the whole upload.
                        let prev = this.uploaded.fetch_add(len, Ordering::Relaxed);
                        let sent = prev + len;
                        // Emit once per `step`-sized band crossed. On an SDK
                        // retry the body replays against the shared counter, so
                        // this may briefly overshoot — harmless: the UI clamps to
                        // 100% and the terminal `done` event resets the total.
                        if sent / *this.step != prev / *this.step {
                            (this.sink)(UploadProgress {
                                key: this.key.clone(),
                                uploaded: sent,
                                total: *this.total,
                                done: false,
                                error: None,
                            });
                        }
                    }
                }
                Poll::Ready(Some(Ok(frame)))
            }
            other => other,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}
