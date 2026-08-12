use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use reqwest::{header::RANGE, Client, StatusCode};
use tokio::{io::AsyncWriteExt, sync::Barrier};
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
    pub(super) async fn run(
        self,
        startup_barrier: Arc<Barrier>,
        startup_ready: Arc<AtomicBool>,
    ) -> Result<(), EngineError> {
        let runtime = Arc::clone(&self.runtime);
        runtime.set_state(SEGMENT_CONNECTING);
        let result = self.transfer(startup_barrier, startup_ready).await;
        match &result {
            Ok(()) => runtime.set_state(SEGMENT_COMPLETED),
            Err(EngineError::Cancelled) => runtime.set_state(SEGMENT_STOPPED),
            Err(error) => runtime.mark_failed(error.to_string()),
        }
        result
    }

    async fn transfer(
        self,
        startup_barrier: Arc<Barrier>,
        startup_ready: Arc<AtomicBool>,
    ) -> Result<(), EngineError> {
        let mut request = apply_source_headers(self.client.get(&self.source.url), &self.source)?
            .header(RANGE, format!("bytes={}-{}", self.start, self.end));
        request = apply_if_range(request, &self.validator)?;
        let mut response = tokio::select! {
            _ = self.cancellation.cancelled() => return Err(EngineError::Cancelled),
            response = request.send() => response.map_err(|_| EngineError::Request)?,
        };
        if response.status() != StatusCode::PARTIAL_CONTENT {
            return Err(match response.status() {
                StatusCode::OK | StatusCode::RANGE_NOT_SATISFIABLE => EngineError::ResumeRejected,
                status => EngineError::SegmentHttpStatus(status.as_u16()),
            });
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
        let first_chunk = tokio::select! {
            _ = self.cancellation.cancelled() => return Err(EngineError::Cancelled),
            chunk = response.chunk() => {
                chunk.map_err(|_| segment_stream_error(self.append, 0))?
            },
        };
        let Some(first_chunk) = first_chunk else {
            return Err(segment_stream_error(self.append, 0));
        };
        tokio::select! {
            _ = self.cancellation.cancelled() => return Err(EngineError::Cancelled),
            _ = startup_barrier.wait() => {}
        }
        startup_ready.store(true, Ordering::Release);
        self.runtime.set_state(SEGMENT_DOWNLOADING);
        let mut output = open_partial(&self.path, self.append).await?;
        let expected_bytes = self.end - self.start + 1;
        let mut received_bytes = first_chunk.len() as u64;
        if received_bytes > expected_bytes {
            return Err(EngineError::InvalidContentRange);
        }
        self.limiter
            .acquire(first_chunk.len(), &self.cancellation)
            .await?;
        output
            .write_all(&first_chunk)
            .await
            .map_err(EngineError::File)?;
        self.runtime.mark_activity(received_bytes);
        loop {
            let next = tokio::select! {
                _ = self.cancellation.cancelled() => return Err(EngineError::Cancelled),
                chunk = response.chunk() => {
                    chunk.map_err(|_| segment_stream_error(self.append, received_bytes))?
                },
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
            return Err(segment_stream_error(self.append, received_bytes));
        }
        output.flush().await.map_err(EngineError::File)?;
        output.sync_data().await.map_err(EngineError::File)
    }
}

pub(super) fn segment_stream_error(append: bool, received_bytes: u64) -> EngineError {
    if append || received_bytes > 0 {
        EngineError::SegmentInterrupted
    } else {
        EngineError::Request
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_initial_segment_rejection_can_degrade_to_one_stream() {
        assert!(matches!(
            segment_stream_error(false, 0),
            EngineError::Request
        ));
        assert!(matches!(
            segment_stream_error(false, 1),
            EngineError::SegmentInterrupted
        ));
        assert!(matches!(
            segment_stream_error(true, 0),
            EngineError::SegmentInterrupted
        ));
    }
}
