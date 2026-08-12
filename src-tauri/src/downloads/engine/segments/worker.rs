use std::{path::PathBuf, sync::Arc};

use reqwest::{header::RANGE, Client, StatusCode};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use super::super::{
    http::{
        apply_if_range, apply_source_headers, response_validator, validate_content_range,
        validate_segment_response_validator,
    },
    rate::BandwidthLimiter,
    storage::open_partial,
    EngineError,
};
use super::{
    SegmentRuntime, SEGMENT_COMPLETED, SEGMENT_CONNECTING, SEGMENT_DOWNLOADING, SEGMENT_STOPPED,
};
use crate::downloads::model::{DownloadSource, SourceValidator};

pub(super) struct SegmentWorker {
    pub(super) client: Client,
    pub(super) source: DownloadSource,
    pub(super) validator: SourceValidator,
    pub(super) total_bytes: u64,
    pub(super) path: PathBuf,
    pub(super) start: u64,
    pub(super) end: u64,
    pub(super) append: bool,
    pub(super) cancellation: CancellationToken,
    pub(super) limiter: BandwidthLimiter,
    pub(super) runtime: Arc<SegmentRuntime>,
}

impl SegmentWorker {
    pub(super) async fn run(self) -> Result<(), EngineError> {
        let runtime = Arc::clone(&self.runtime);
        runtime.set_state(SEGMENT_CONNECTING);
        let result = self.transfer().await;
        match &result {
            Ok(()) => runtime.set_state(SEGMENT_COMPLETED),
            Err(EngineError::Cancelled) => runtime.set_state(SEGMENT_STOPPED),
            Err(error) => runtime.mark_failed(error.to_string()),
        }
        result
    }

    async fn transfer(self) -> Result<(), EngineError> {
        let mut request = apply_source_headers(self.client.get(&self.source.url), &self.source)?
            .header(RANGE, format!("bytes={}-{}", self.start, self.end));
        request = apply_if_range(request, &self.validator)?;
        let mut response = tokio::select! {
            _ = self.cancellation.cancelled() => return Err(EngineError::Cancelled),
            response = request.send() => response.map_err(|_| EngineError::Request)?,
        };
        if response.status() != StatusCode::PARTIAL_CONTENT {
            return Err(EngineError::ResumeRejected);
        }
        validate_content_range(
            response.headers(),
            self.start,
            Some(self.end),
            Some(self.total_bytes),
        )?;
        validate_segment_response_validator(
            &self.validator,
            &response_validator(response.headers()),
        )?;
        self.runtime.set_state(SEGMENT_DOWNLOADING);
        let mut output = open_partial(&self.path, self.append).await?;
        let expected_bytes = self.end - self.start + 1;
        let mut received_bytes = 0_u64;
        loop {
            let next = tokio::select! {
                _ = self.cancellation.cancelled() => return Err(EngineError::Cancelled),
                chunk = response.chunk() => chunk.map_err(|_| EngineError::Request)?,
            };
            let Some(chunk) = next else { break };
            self.limiter
                .acquire(chunk.len(), &self.cancellation)
                .await?;
            received_bytes += chunk.len() as u64;
            if received_bytes > expected_bytes {
                return Err(EngineError::InvalidContentRange);
            }
            output.write_all(&chunk).await.map_err(EngineError::File)?;
            self.runtime.mark_activity(chunk.len() as u64);
        }
        if received_bytes != expected_bytes {
            return Err(EngineError::Request);
        }
        output.flush().await.map_err(EngineError::File)?;
        output
            .get_ref()
            .sync_data()
            .await
            .map_err(EngineError::File)
    }
}
