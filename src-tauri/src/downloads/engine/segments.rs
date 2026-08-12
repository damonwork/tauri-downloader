use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicI64, AtomicU64, AtomicU8, Ordering},
        Arc, Mutex as StdMutex,
    },
    time::Instant,
};

use chrono::{DateTime, Utc};
use reqwest::{header::RANGE, Client, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::{fs, io::AsyncWriteExt, sync::watch, task::JoinSet, time};
use tokio_util::sync::CancellationToken;

use super::http::{
    apply_if_range, apply_source_headers, confirm_segment_source, response_validator,
    validate_content_range, validate_segment_response_validator, ProbeResult,
};
use super::rate::{BandwidthLimiter, TransferRateEstimator};
use super::storage::{
    ensure_destination_available, file_len, finalize_partial, merge_segments, open_partial,
    partial_path, persist_segment_metadata, read_segment_metadata, remove_segment_metadata,
    require_segment_metadata, segment_metadata_path, segment_path,
};
use super::{
    ensure_segment_partials_compatible, send_progress, EngineError, EngineInput, EngineOutput,
    EngineProgress, MIN_SEGMENT_SIZE, PROGRESS_INTERVAL,
};
use crate::downloads::model::{
    DownloadSource, ResumeSupport, SegmentProgress, SegmentState, SourceValidator, TransferMode,
    TransferPhase, TransferSize, TransferTelemetry,
};

const SEGMENT_PENDING: u8 = 0;
const SEGMENT_CONNECTING: u8 = 1;
pub(super) const SEGMENT_DOWNLOADING: u8 = 2;
pub(super) const SEGMENT_COMPLETED: u8 = 3;
const SEGMENT_FAILED: u8 = 4;
const SEGMENT_STOPPED: u8 = 5;

#[derive(Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SegmentMetadata {
    pub(super) total_bytes: u64,
    pub(super) validator: SourceValidator,
    pub(super) threads: u8,
    pub(super) ranges: Vec<(u64, u64)>,
}

pub(super) struct SegmentRuntime {
    index: u8,
    start_byte: u64,
    end_byte: u64,
    downloaded_bytes: AtomicU64,
    state: AtomicU8,
    last_activity_ms: AtomicI64,
    error: StdMutex<Option<String>>,
}

impl SegmentRuntime {
    pub(super) fn new(index: usize, start_byte: u64, end_byte: u64, downloaded_bytes: u64) -> Self {
        Self {
            index: index as u8,
            start_byte,
            end_byte,
            downloaded_bytes: AtomicU64::new(downloaded_bytes),
            state: AtomicU8::new(if downloaded_bytes == end_byte - start_byte + 1 {
                SEGMENT_COMPLETED
            } else {
                SEGMENT_PENDING
            }),
            last_activity_ms: AtomicI64::new(0),
            error: StdMutex::new(None),
        }
    }

    pub(super) fn set_state(&self, state: u8) {
        self.state.store(state, Ordering::Relaxed);
    }

    pub(super) fn mark_activity(&self, bytes: u64) {
        self.downloaded_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.last_activity_ms
            .store(Utc::now().timestamp_millis(), Ordering::Relaxed);
    }

    fn mark_failed(&self, message: String) {
        self.set_state(SEGMENT_FAILED);
        if let Ok(mut error) = self.error.lock() {
            *error = Some(message);
        }
    }

    pub(super) fn snapshot(&self, speed_bytes: u64) -> SegmentProgress {
        self.snapshot_at(self.downloaded_bytes.load(Ordering::Relaxed), speed_bytes)
    }

    fn snapshot_at(&self, downloaded_bytes: u64, speed_bytes: u64) -> SegmentProgress {
        let last_activity_ms = self.last_activity_ms.load(Ordering::Relaxed);
        let state = match self.state.load(Ordering::Relaxed) {
            SEGMENT_CONNECTING => SegmentState::Connecting,
            SEGMENT_DOWNLOADING => SegmentState::Downloading,
            SEGMENT_COMPLETED => SegmentState::Completed,
            SEGMENT_FAILED => SegmentState::Failed,
            SEGMENT_STOPPED => SegmentState::Stopped,
            _ => SegmentState::Pending,
        };
        SegmentProgress {
            index: self.index,
            start_byte: self.start_byte,
            end_byte: Some(self.end_byte),
            downloaded_bytes,
            speed_bytes: if matches!(state, SegmentState::Downloading) {
                speed_bytes
            } else {
                0
            },
            state,
            last_activity_at: DateTime::from_timestamp_millis(last_activity_ms),
            error: self.error.lock().ok().and_then(|error| error.clone()),
        }
    }
}

pub(super) async fn run_segmented(
    input: EngineInput,
    client: Client,
    mut probe: ProbeResult,
    has_segment_partials: bool,
    cancellation: CancellationToken,
    progress: watch::Sender<Option<EngineProgress>>,
) -> Result<EngineOutput, EngineError> {
    let TransferSize::Known { total_bytes } = &probe.size else {
        return Err(EngineError::InvalidContentRange);
    };
    let total_bytes = *total_bytes;
    let final_path = input.destination_dir.join(&input.item.file_name);
    ensure_destination_available(&final_path).await?;
    let metadata_path = segment_metadata_path(&input.destination_dir, &input.item.file_name);
    let ranges = split_ranges(total_bytes, input.item.threads);
    let stored_metadata = if has_segment_partials {
        read_segment_metadata(&metadata_path).await?
    } else {
        None
    };
    require_segment_metadata(has_segment_partials, stored_metadata.as_ref())?;
    if stored_metadata
        .as_ref()
        .is_some_and(|metadata| metadata.threads != input.item.threads || metadata.ranges != ranges)
    {
        return Err(EngineError::SourceChanged);
    }
    let (previous_size, previous_validator) = stored_metadata.as_ref().map_or_else(
        || {
            (
                input.item.transfer.size.clone(),
                input.item.transfer.validator.clone(),
            )
        },
        |metadata| {
            (
                TransferSize::Known {
                    total_bytes: metadata.total_bytes,
                },
                metadata.validator.clone(),
            )
        },
    );
    if has_segment_partials
        && matches!(&probe.validator, SourceValidator::None)
        && !matches!(&previous_validator, SourceValidator::None)
    {
        probe.validator = previous_validator.clone();
    }
    probe = confirm_segment_source(&client, &input.item.source, probe, &cancellation).await?;
    if has_segment_partials {
        ensure_segment_partials_compatible(
            &previous_size,
            &probe.size,
            &previous_validator,
            &probe.validator,
        )?;
    }
    persist_segment_metadata(
        &metadata_path,
        &SegmentMetadata {
            total_bytes,
            validator: probe.validator.clone(),
            threads: input.item.threads,
            ranges: ranges.clone(),
        },
    )
    .await?;

    let limiter = BandwidthLimiter::new(input.item.speed_limit_bytes);
    let size = TransferSize::Known { total_bytes };
    let resume = ResumeSupport::Supported;
    let mode = TransferMode::Segmented;
    let mut tasks = JoinSet::new();
    let mut runtimes = Vec::with_capacity(ranges.len());
    let mut workers = Vec::with_capacity(ranges.len());
    for (index, (start, end)) in ranges.iter().copied().enumerate() {
        let segment_path = segment_path(&input.destination_dir, &input.item.file_name, index);
        let mut existing = file_len(&segment_path).await?;
        let expected = end - start + 1;
        if existing > expected {
            fs::remove_file(&segment_path)
                .await
                .map_err(EngineError::File)?;
            existing = 0;
        }
        let resumed = existing.min(expected);
        let runtime = Arc::new(SegmentRuntime::new(index, start, end, resumed));
        runtimes.push(Arc::clone(&runtime));
        if resumed != expected {
            workers.push(SegmentWorker {
                client: client.clone(),
                source: input.item.source.clone(),
                validator: probe.validator.clone(),
                total_bytes,
                path: segment_path,
                start: start + resumed,
                end,
                append: resumed > 0,
                cancellation: cancellation.child_token(),
                limiter: limiter.clone(),
                runtime,
            });
        }
    }

    let initial = snapshot_segments(&runtimes, &vec![0; runtimes.len()]);
    let initial_downloaded = segments_downloaded(&initial);
    publish_progress(
        &progress,
        initial_downloaded,
        &size,
        &probe.validator,
        &resume,
        0,
        TransferPhase::Connecting,
        mode.clone(),
        initial,
    );
    for worker in workers {
        tasks.spawn(worker.run());
    }

    let mut interval = time::interval(PROGRESS_INTERVAL);
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    interval.tick().await;
    let sampled_at = Instant::now();
    let baselines = runtimes
        .iter()
        .map(|runtime| runtime.downloaded_bytes.load(Ordering::Relaxed))
        .collect::<Vec<_>>();
    let mut segment_rates = baselines
        .iter()
        .map(|downloaded| TransferRateEstimator::new(*downloaded, sampled_at))
        .collect::<Vec<_>>();
    let mut aggregate_rate =
        TransferRateEstimator::new(baselines.iter().copied().sum(), sampled_at);

    while !tasks.is_empty() {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                abort_segments(&mut tasks).await;
                stop_active_segments(&runtimes);
                let segments = snapshot_segments(&runtimes, &vec![0; runtimes.len()]);
                publish_progress(&progress, segments_downloaded(&segments), &size, &probe.validator, &resume, 0, TransferPhase::Idle, mode.clone(), segments);
                return Err(EngineError::Cancelled);
            }
            _ = interval.tick() => {
                let sampled_at = Instant::now();
                let segments = sample_segments(&runtimes, &mut segment_rates, sampled_at);
                let current = segments_downloaded(&segments);
                let speed = aggregate_rate.sample(current, sampled_at);
                let phase = if segments.iter().any(|segment| matches!(segment.state, SegmentState::Downloading)) {
                    TransferPhase::Transferring
                } else {
                    TransferPhase::Connecting
                };
                publish_progress(&progress, current, &size, &probe.validator, &resume, speed, phase, mode.clone(), segments);
            }
            result = tasks.join_next() => match result {
                Some(Ok(Ok(()))) => {}
                Some(Ok(Err(error))) => {
                    publish_stopped_segments(&mut tasks, &runtimes, &progress, &size, &probe.validator, &resume, &mode).await;
                    return Err(error);
                }
                Some(Err(_)) => {
                    publish_stopped_segments(&mut tasks, &runtimes, &progress, &size, &probe.validator, &resume, &mode).await;
                    return Err(EngineError::SegmentTask);
                }
                None => break,
            }
        }
    }

    let completed_segments = snapshot_segments(&runtimes, &vec![0; runtimes.len()]);
    publish_progress(
        &progress,
        total_bytes,
        &size,
        &probe.validator,
        &resume,
        0,
        TransferPhase::Merging,
        mode.clone(),
        completed_segments.clone(),
    );
    let partial_path = partial_path(&input.destination_dir, &input.item.file_name);
    merge_segments(
        &partial_path,
        &input.destination_dir,
        &input.item.file_name,
        ranges.len(),
        &cancellation,
    )
    .await?;
    let _ = remove_segment_metadata(&metadata_path).await;
    publish_progress(
        &progress,
        total_bytes,
        &size,
        &probe.validator,
        &resume,
        0,
        TransferPhase::Finalizing,
        mode.clone(),
        completed_segments.clone(),
    );
    finalize_partial(&partial_path, &final_path).await?;
    let telemetry = segmented_telemetry(TransferPhase::Finalizing, mode, completed_segments);
    send_progress(
        &progress,
        total_bytes,
        &size,
        &probe.validator,
        &resume,
        0,
        telemetry.clone(),
    );
    Ok(EngineOutput {
        downloaded_bytes: total_bytes,
        size,
        validator: probe.validator,
        resume,
        telemetry,
    })
}

struct SegmentWorker {
    client: Client,
    source: DownloadSource,
    validator: SourceValidator,
    total_bytes: u64,
    path: PathBuf,
    start: u64,
    end: u64,
    append: bool,
    cancellation: CancellationToken,
    limiter: BandwidthLimiter,
    runtime: Arc<SegmentRuntime>,
}

impl SegmentWorker {
    async fn run(self) -> Result<(), EngineError> {
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

async fn publish_stopped_segments(
    tasks: &mut JoinSet<Result<(), EngineError>>,
    runtimes: &[Arc<SegmentRuntime>],
    progress: &watch::Sender<Option<EngineProgress>>,
    size: &TransferSize,
    validator: &SourceValidator,
    resume: &ResumeSupport,
    mode: &TransferMode,
) {
    abort_segments(tasks).await;
    stop_active_segments(runtimes);
    let segments = snapshot_segments(runtimes, &vec![0; runtimes.len()]);
    publish_progress(
        progress,
        segments_downloaded(&segments),
        size,
        validator,
        resume,
        0,
        TransferPhase::Transferring,
        mode.clone(),
        segments,
    );
}

#[allow(clippy::too_many_arguments)]
fn publish_progress(
    progress: &watch::Sender<Option<EngineProgress>>,
    downloaded: u64,
    size: &TransferSize,
    validator: &SourceValidator,
    resume: &ResumeSupport,
    speed: u64,
    phase: TransferPhase,
    mode: TransferMode,
    segments: Vec<SegmentProgress>,
) {
    send_progress(
        progress,
        downloaded,
        size,
        validator,
        resume,
        speed,
        segmented_telemetry(phase, mode, segments),
    );
}

pub(super) fn split_ranges(total: u64, requested_threads: u8) -> Vec<(u64, u64)> {
    let max_useful = total.div_ceil(MIN_SEGMENT_SIZE).max(1);
    let count = u64::from(requested_threads).min(max_useful).max(1);
    let segment_size = total.div_ceil(count);
    (0..count)
        .map(|index| {
            let start = index * segment_size;
            let end = ((index + 1) * segment_size).min(total) - 1;
            (start, end)
        })
        .collect()
}

async fn abort_segments(tasks: &mut JoinSet<Result<(), EngineError>>) {
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
}

fn stop_active_segments(runtimes: &[Arc<SegmentRuntime>]) {
    for runtime in runtimes {
        if matches!(
            runtime.state.load(Ordering::Relaxed),
            SEGMENT_PENDING | SEGMENT_CONNECTING | SEGMENT_DOWNLOADING
        ) {
            runtime.set_state(SEGMENT_STOPPED);
        }
    }
}

fn segmented_telemetry(
    phase: TransferPhase,
    mode: TransferMode,
    segments: Vec<SegmentProgress>,
) -> TransferTelemetry {
    TransferTelemetry {
        phase,
        mode,
        segments,
    }
}

fn snapshot_segments(runtimes: &[Arc<SegmentRuntime>], speeds: &[u64]) -> Vec<SegmentProgress> {
    runtimes
        .iter()
        .enumerate()
        .map(|(index, runtime)| runtime.snapshot(*speeds.get(index).unwrap_or(&0)))
        .collect()
}

pub(super) fn segments_downloaded(segments: &[SegmentProgress]) -> u64 {
    segments
        .iter()
        .map(|segment| segment.downloaded_bytes)
        .sum()
}

fn sample_segments(
    runtimes: &[Arc<SegmentRuntime>],
    rates: &mut [TransferRateEstimator],
    sampled_at: Instant,
) -> Vec<SegmentProgress> {
    runtimes
        .iter()
        .enumerate()
        .map(|(index, runtime)| {
            let current = runtime.downloaded_bytes.load(Ordering::Relaxed);
            let speed = rates
                .get_mut(index)
                .map_or(0, |rate| rate.sample(current, sampled_at));
            runtime.snapshot_at(current, speed)
        })
        .collect()
}
