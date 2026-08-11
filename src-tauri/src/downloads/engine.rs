use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicI64, AtomicU64, AtomicU8, Ordering},
        Arc, Mutex as StdMutex,
    },
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use reqwest::{
    header::{
        HeaderMap, HeaderName, HeaderValue, ACCEPT_ENCODING, ACCEPT_RANGES, CONTENT_DISPOSITION,
        CONTENT_LENGTH, CONTENT_RANGE, COOKIE, ETAG, IF_RANGE, LAST_MODIFIED, RANGE,
    },
    redirect::Policy,
    Client, RequestBuilder, StatusCode,
};
use thiserror::Error;
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncWriteExt, BufWriter},
    sync::{watch, Mutex},
    task::JoinSet,
    time,
};
use tokio_util::sync::CancellationToken;

use super::model::{
    DownloadItem, DownloadSource, ResumeSupport, SegmentProgress, SegmentState, SourceValidator,
    TransferMode, TransferPhase, TransferSize, TransferTelemetry,
};

const MAX_REDIRECTS: usize = 10;
const MIN_SEGMENT_SIZE: u64 = 2 * 1024 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(300);
const METADATA_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:153.0) Gecko/20100101 Firefox/153.0";

#[derive(Clone, Debug)]
pub enum ResolvedProxy {
    Direct,
    Url(String),
}

#[derive(Clone, Debug)]
pub struct EngineInput {
    pub item: DownloadItem,
    pub destination_dir: PathBuf,
    pub proxy: ResolvedProxy,
}

#[derive(Clone, Debug)]
pub struct EngineProgress {
    pub downloaded_bytes: u64,
    pub size: TransferSize,
    pub validator: SourceValidator,
    pub resume: ResumeSupport,
    pub speed_bytes: u64,
    pub telemetry: TransferTelemetry,
}

#[derive(Clone, Debug)]
pub struct EngineOutput {
    pub downloaded_bytes: u64,
    pub size: TransferSize,
    pub validator: SourceValidator,
    pub resume: ResumeSupport,
    pub telemetry: TransferTelemetry,
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Transferencia cancelada")]
    Cancelled,
    #[error("No se pudo completar la comunicación con el servidor")]
    Request,
    #[error("El servidor devolvió un estado HTTP inesperado: {0}")]
    HttpStatus(u16),
    #[error(
        "El servidor no respetó el rango solicitado; reinicia la descarga para evitar corrupción"
    )]
    ResumeRejected,
    #[error("El contenido del enlace cambió; actualiza el enlace o reinicia la descarga")]
    SourceChanged,
    #[error("El servidor devolvió un Content-Range inválido")]
    InvalidContentRange,
    #[error("Header inválido: {0}")]
    InvalidHeader(String),
    #[error("El archivo de destino ya existe")]
    DestinationExists,
    #[error("No se pudo escribir el archivo parcial")]
    File(#[source] std::io::Error),
    #[error("Uno de los segmentos de descarga falló")]
    SegmentTask,
}

impl EngineError {
    pub fn recoverable(&self) -> bool {
        matches!(
            self,
            Self::Request | Self::HttpStatus(_) | Self::SegmentTask
        )
    }
}

pub struct DownloadEngine;

impl DownloadEngine {
    pub async fn detect_file_name(
        source: &DownloadSource,
        proxy: &ResolvedProxy,
    ) -> Option<String> {
        let client = build_client(proxy, source).ok()?;
        let request = apply_source_headers(client.head(&source.url), source).ok()?;
        let response = time::timeout(METADATA_TIMEOUT, request.send())
            .await
            .ok()?
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        content_disposition_file_name(response.headers())
            .or_else(|| file_name_from_response_url(response.url()))
    }

    pub async fn run(
        input: EngineInput,
        cancellation: CancellationToken,
        progress: watch::Sender<Option<EngineProgress>>,
    ) -> Result<EngineOutput, EngineError> {
        send_progress(
            &progress,
            input.item.transfer.downloaded_bytes,
            &input.item.transfer.size,
            &input.item.transfer.validator,
            &input.item.transfer.resume,
            0,
            TransferTelemetry {
                phase: TransferPhase::Preparing,
                ..TransferTelemetry::default()
            },
        );
        fs::create_dir_all(&input.destination_dir)
            .await
            .map_err(EngineError::File)?;
        recover_interrupted_finalization(&input.destination_dir, &input.item.file_name).await?;

        let client = build_client(&input.proxy, &input.item.source)?;
        let single_partial = partial_path(&input.destination_dir, &input.item.file_name);
        if file_len(&single_partial).await? > 0 {
            return run_single(
                input,
                client,
                cancellation,
                progress,
                Some("Se encontró un parcial creado como flujo único".to_owned()),
            )
            .await;
        }

        let has_segment_partials = has_segment_partials(
            &input.destination_dir,
            &input.item.file_name,
            input.item.threads,
        )
        .await?;
        send_progress(
            &progress,
            input.item.transfer.downloaded_bytes,
            &input.item.transfer.size,
            &input.item.transfer.validator,
            &input.item.transfer.resume,
            0,
            TransferTelemetry {
                phase: TransferPhase::Probing,
                ..TransferTelemetry::default()
            },
        );
        let probe = probe_source(&client, &input.item.source, &cancellation).await;

        if let Some(probe) = probe.clone() {
            let can_segment = probe.accepts_ranges
                && input.item.threads > 1
                && !matches!(&probe.validator, SourceValidator::None)
                && matches!(&probe.size, TransferSize::Known { total_bytes } if *total_bytes >= MIN_SEGMENT_SIZE);
            if can_segment {
                ensure_same_source(
                    u64::from(has_segment_partials),
                    &input.item.transfer.validator,
                    &probe.validator,
                )?;
                let segmented = run_segmented(
                    input.clone(),
                    client.clone(),
                    probe,
                    cancellation.clone(),
                    progress.clone(),
                )
                .await;
                match segmented {
                    Ok(output) => return Ok(output),
                    Err(error) if segmented_failure_allows_single_stream(&error) => {
                        remove_segment_partials(
                            &input.destination_dir,
                            &input.item.file_name,
                            input.item.threads,
                        )
                        .await?;
                        return run_single(
                            input,
                            client,
                            cancellation,
                            progress,
                            Some(
                                "El servidor rechazó la transferencia segmentada; se usa un flujo"
                                    .to_owned(),
                            ),
                        )
                        .await;
                    }
                    Err(error) => return Err(error),
                }
            }
        }

        if has_segment_partials {
            return Err(EngineError::ResumeRejected);
        }

        let reason = single_stream_reason(probe.as_ref(), input.item.threads);
        run_single(input, client, cancellation, progress, Some(reason)).await
    }
}

#[derive(Clone)]
struct ProbeResult {
    size: TransferSize,
    validator: SourceValidator,
    accepts_ranges: bool,
}

const SEGMENT_PENDING: u8 = 0;
const SEGMENT_CONNECTING: u8 = 1;
const SEGMENT_DOWNLOADING: u8 = 2;
const SEGMENT_COMPLETED: u8 = 3;
const SEGMENT_FAILED: u8 = 4;
const SEGMENT_STOPPED: u8 = 5;

struct SegmentRuntime {
    index: u8,
    start_byte: u64,
    end_byte: u64,
    downloaded_bytes: AtomicU64,
    state: AtomicU8,
    last_activity_ms: AtomicI64,
    error: StdMutex<Option<String>>,
}

impl SegmentRuntime {
    fn new(index: usize, start_byte: u64, end_byte: u64, downloaded_bytes: u64) -> Self {
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

    fn set_state(&self, state: u8) {
        self.state.store(state, Ordering::Relaxed);
    }

    fn mark_activity(&self, bytes: u64) {
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

    fn snapshot(&self, speed_bytes: u64) -> SegmentProgress {
        let last_activity_ms = self.last_activity_ms.load(Ordering::Relaxed);
        SegmentProgress {
            index: self.index,
            start_byte: self.start_byte,
            end_byte: Some(self.end_byte),
            downloaded_bytes: self.downloaded_bytes.load(Ordering::Relaxed),
            speed_bytes,
            state: match self.state.load(Ordering::Relaxed) {
                SEGMENT_CONNECTING => SegmentState::Connecting,
                SEGMENT_DOWNLOADING => SegmentState::Downloading,
                SEGMENT_COMPLETED => SegmentState::Completed,
                SEGMENT_FAILED => SegmentState::Failed,
                SEGMENT_STOPPED => SegmentState::Stopped,
                _ => SegmentState::Pending,
            },
            last_activity_at: DateTime::from_timestamp_millis(last_activity_ms),
            error: self.error.lock().ok().and_then(|error| error.clone()),
        }
    }
}

#[derive(Clone)]
struct BandwidthLimiter {
    bytes_per_second: u64,
    next_available: Arc<Mutex<Instant>>,
}

impl BandwidthLimiter {
    fn new(bytes_per_second: u64) -> Self {
        Self {
            bytes_per_second,
            next_available: Arc::new(Mutex::new(Instant::now())),
        }
    }

    async fn acquire(
        &self,
        bytes: usize,
        cancellation: &CancellationToken,
    ) -> Result<(), EngineError> {
        if self.bytes_per_second == 0 || bytes == 0 {
            return Ok(());
        }
        let wait = {
            let mut next_available = self.next_available.lock().await;
            let now = Instant::now();
            if *next_available < now {
                *next_available = now;
            }
            *next_available += transfer_duration(bytes, self.bytes_per_second);
            next_available.saturating_duration_since(now)
        };
        if wait.is_zero() {
            return Ok(());
        }
        tokio::select! {
            _ = cancellation.cancelled() => Err(EngineError::Cancelled),
            _ = time::sleep(wait) => Ok(()),
        }
    }
}

async fn probe_source(
    client: &Client,
    source: &DownloadSource,
    cancellation: &CancellationToken,
) -> Option<ProbeResult> {
    let request = apply_source_headers(client.head(&source.url), source).ok()?;
    let response = tokio::select! {
        _ = cancellation.cancelled() => return None,
        response = request.send() => response.ok()?,
    };
    if !response.status().is_success() {
        return None;
    }
    let size = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(TransferSize::Unknown, |total_bytes| TransferSize::Known {
            total_bytes,
        });
    let accepts_ranges = accepts_ranges(response.headers());
    Some(ProbeResult {
        size,
        validator: response_validator(response.headers()),
        accepts_ranges,
    })
}

fn single_stream_reason(probe: Option<&ProbeResult>, requested_threads: u8) -> String {
    if requested_threads <= 1 {
        return "Configurado para un único flujo".to_owned();
    }
    let Some(probe) = probe else {
        return "No se pudo confirmar de forma segura el soporte de segmentos".to_owned();
    };
    if !probe.accepts_ranges {
        return "El servidor no admite solicitudes por rango".to_owned();
    }
    if matches!(&probe.validator, SourceValidator::None) {
        return "El servidor no proporciona un validador seguro".to_owned();
    }
    match &probe.size {
        TransferSize::Unknown => "El servidor no informó el tamaño del archivo".to_owned(),
        TransferSize::Known { total_bytes } if *total_bytes < MIN_SEGMENT_SIZE => {
            "El archivo es demasiado pequeño para dividirlo".to_owned()
        }
        _ => "Se seleccionó un único flujo".to_owned(),
    }
}

async fn run_single(
    input: EngineInput,
    client: Client,
    cancellation: CancellationToken,
    progress: watch::Sender<Option<EngineProgress>>,
    reason: Option<String>,
) -> Result<EngineOutput, EngineError> {
    let final_path = input.destination_dir.join(&input.item.file_name);
    ensure_destination_available(&final_path).await?;
    let partial_path = partial_path(&input.destination_dir, &input.item.file_name);
    let resume_at = file_len(&partial_path).await?;

    let mut request = apply_source_headers(client.get(&input.item.source.url), &input.item.source)?;
    if resume_at > 0 {
        request = request.header(RANGE, format!("bytes={resume_at}-"));
        request = apply_if_range(request, &input.item.transfer.validator)?;
    }

    send_progress(
        &progress,
        resume_at,
        &input.item.transfer.size,
        &input.item.transfer.validator,
        &input.item.transfer.resume,
        0,
        single_telemetry(
            TransferPhase::Connecting,
            reason.clone(),
            &input.item.transfer.size,
            resume_at,
            0,
            SegmentState::Connecting,
            None,
        ),
    );

    let mut response = tokio::select! {
        _ = cancellation.cancelled() => return Err(EngineError::Cancelled),
        response = request.send() => response.map_err(|_| EngineError::Request)?,
    };
    if resume_at > 0 && response.status() != StatusCode::PARTIAL_CONTENT {
        return Err(EngineError::ResumeRejected);
    }
    if !response.status().is_success() {
        return Err(EngineError::HttpStatus(response.status().as_u16()));
    }
    if resume_at > 0 {
        let total_bytes =
            content_range_total(response.headers()).ok_or(EngineError::InvalidContentRange)?;
        let expected_total = match &input.item.transfer.size {
            TransferSize::Known { total_bytes } => Some(*total_bytes),
            TransferSize::Unknown => Some(total_bytes),
        };
        validate_content_range(
            response.headers(),
            resume_at,
            Some(total_bytes.saturating_sub(1)),
            expected_total,
        )?;
    }

    let validator = response_validator(response.headers());
    ensure_same_source(resume_at, &input.item.transfer.validator, &validator)?;
    let validator = if resume_at > 0 && matches!(&validator, SourceValidator::None) {
        input.item.transfer.validator.clone()
    } else {
        validator
    };
    let resume = resume_support(
        response.status() == StatusCode::PARTIAL_CONTENT || accepts_ranges(response.headers()),
    );
    let size = response_size(response.headers(), resume_at);
    ensure_same_size(resume_at, &input.item.transfer.size, &size)?;
    let mut output = open_partial(&partial_path, resume_at > 0).await?;
    let limiter = BandwidthLimiter::new(input.item.speed_limit_bytes);
    let mut downloaded = resume_at;
    let mut checkpoint_bytes = downloaded;
    let mut checkpoint_at = Instant::now();
    let mut last_activity_at = None;
    let mut interval = time::interval(PROGRESS_INTERVAL);
    interval.tick().await;

    send_progress(
        &progress,
        downloaded,
        &size,
        &validator,
        &resume,
        0,
        single_telemetry(
            TransferPhase::Transferring,
            reason.clone(),
            &size,
            downloaded,
            0,
            SegmentState::Downloading,
            last_activity_at,
        ),
    );
    loop {
        let next = tokio::select! {
            _ = cancellation.cancelled() => return Err(EngineError::Cancelled),
            _ = interval.tick() => {
                let speed = bytes_per_second(downloaded.saturating_sub(checkpoint_bytes), checkpoint_at.elapsed());
                send_progress(
                    &progress,
                    downloaded,
                    &size,
                    &validator,
                    &resume,
                    speed,
                    single_telemetry(
                        TransferPhase::Transferring,
                        reason.clone(),
                        &size,
                        downloaded,
                        speed,
                        SegmentState::Downloading,
                        last_activity_at,
                    ),
                );
                checkpoint_bytes = downloaded;
                checkpoint_at = Instant::now();
                continue;
            }
            chunk = response.chunk() => chunk.map_err(|_| EngineError::Request)?,
        };
        let Some(chunk) = next else { break };
        limiter.acquire(chunk.len(), &cancellation).await?;
        output.write_all(&chunk).await.map_err(EngineError::File)?;
        downloaded += chunk.len() as u64;
        last_activity_at = Some(Utc::now());
    }

    send_progress(
        &progress,
        downloaded,
        &size,
        &validator,
        &resume,
        0,
        single_telemetry(
            TransferPhase::Finalizing,
            reason.clone(),
            &size,
            downloaded,
            0,
            SegmentState::Downloading,
            last_activity_at,
        ),
    );
    output.flush().await.map_err(EngineError::File)?;
    output
        .get_ref()
        .sync_all()
        .await
        .map_err(EngineError::File)?;
    verify_complete(downloaded, &size)?;
    finalize_partial(&partial_path, &final_path).await?;
    let telemetry = single_telemetry(
        TransferPhase::Finalizing,
        reason,
        &size,
        downloaded,
        0,
        SegmentState::Completed,
        last_activity_at,
    );
    send_progress(
        &progress,
        downloaded,
        &size,
        &validator,
        &resume,
        0,
        telemetry.clone(),
    );
    Ok(EngineOutput {
        downloaded_bytes: downloaded,
        size,
        validator,
        resume,
        telemetry,
    })
}

async fn run_segmented(
    input: EngineInput,
    client: Client,
    probe: ProbeResult,
    cancellation: CancellationToken,
    progress: watch::Sender<Option<EngineProgress>>,
) -> Result<EngineOutput, EngineError> {
    let TransferSize::Known { total_bytes } = probe.size else {
        return Err(EngineError::InvalidContentRange);
    };
    let final_path = input.destination_dir.join(&input.item.file_name);
    ensure_destination_available(&final_path).await?;
    let ranges = split_ranges(total_bytes, input.item.threads);
    let limiter = BandwidthLimiter::new(input.item.speed_limit_bytes);
    let mut tasks = JoinSet::new();
    let size = TransferSize::Known { total_bytes };
    let resume = ResumeSupport::Supported;
    let mode = TransferMode::Segmented;
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
        if resumed == expected {
            continue;
        }

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

    let initial = snapshot_segments(&runtimes, &vec![0; runtimes.len()]);
    let initial_downloaded = segments_downloaded(&initial);
    send_progress(
        &progress,
        initial_downloaded,
        &size,
        &probe.validator,
        &resume,
        0,
        segmented_telemetry(TransferPhase::Connecting, mode.clone(), initial),
    );
    for worker in workers {
        tasks.spawn(worker.run());
    }

    let mut interval = time::interval(PROGRESS_INTERVAL);
    let mut segment_checkpoints = runtimes
        .iter()
        .map(|runtime| runtime.downloaded_bytes.load(Ordering::Relaxed))
        .collect::<Vec<_>>();
    let mut checkpoint_at = Instant::now();

    while !tasks.is_empty() {
        tokio::select! {
            _ = cancellation.cancelled() => {
                abort_segments(&mut tasks).await;
                stop_active_segments(&runtimes);
                return Err(EngineError::Cancelled);
            }
            _ = interval.tick() => {
                let elapsed = checkpoint_at.elapsed();
                let segments = sample_segments(&runtimes, &mut segment_checkpoints, elapsed);
                let current = segments_downloaded(&segments);
                let speed = segments_speed(&segments);
                let phase = if segments.iter().any(|segment| matches!(segment.state, SegmentState::Downloading)) {
                    TransferPhase::Transferring
                } else {
                    TransferPhase::Connecting
                };
                send_progress(
                    &progress,
                    current,
                    &size,
                    &probe.validator,
                    &resume,
                    speed,
                    segmented_telemetry(phase, mode.clone(), segments),
                );
                checkpoint_at = Instant::now();
            }
            result = tasks.join_next() => {
                match result {
                    Some(Ok(Ok(()))) => {}
                    Some(Ok(Err(error))) => {
                        abort_segments(&mut tasks).await;
                        stop_active_segments(&runtimes);
                        let segments = snapshot_segments(&runtimes, &vec![0; runtimes.len()]);
                        send_progress(
                            &progress,
                            segments_downloaded(&segments),
                            &size,
                            &probe.validator,
                            &resume,
                            0,
                            segmented_telemetry(
                                TransferPhase::Transferring,
                                mode.clone(),
                                segments,
                            ),
                        );
                        return Err(error);
                    }
                    Some(Err(_)) => {
                        abort_segments(&mut tasks).await;
                        stop_active_segments(&runtimes);
                        let segments = snapshot_segments(&runtimes, &vec![0; runtimes.len()]);
                        send_progress(
                            &progress,
                            segments_downloaded(&segments),
                            &size,
                            &probe.validator,
                            &resume,
                            0,
                            segmented_telemetry(
                                TransferPhase::Transferring,
                                mode.clone(),
                                segments,
                            ),
                        );
                        return Err(EngineError::SegmentTask);
                    }
                    None => break,
                }
            }
        }
    }

    let completed_segments = snapshot_segments(&runtimes, &vec![0; runtimes.len()]);
    send_progress(
        &progress,
        total_bytes,
        &size,
        &probe.validator,
        &resume,
        0,
        segmented_telemetry(
            TransferPhase::Merging,
            mode.clone(),
            completed_segments.clone(),
        ),
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
    send_progress(
        &progress,
        total_bytes,
        &size,
        &probe.validator,
        &resume,
        0,
        segmented_telemetry(
            TransferPhase::Finalizing,
            mode.clone(),
            completed_segments.clone(),
        ),
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
        if response_validator(response.headers()) != self.validator {
            return Err(EngineError::SourceChanged);
        }
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
            .map_err(EngineError::File)?;
        Ok(())
    }
}

fn build_client(proxy: &ResolvedProxy, source: &DownloadSource) -> Result<Client, EngineError> {
    let carries_credentials = !source.headers.is_empty() || !source.cookies.is_empty();
    let mut default_headers = HeaderMap::new();
    default_headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    let mut builder = Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .default_headers(default_headers)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(45))
        .redirect(if carries_credentials {
            Policy::none()
        } else {
            Policy::limited(MAX_REDIRECTS)
        });
    match proxy {
        ResolvedProxy::Direct => builder = builder.no_proxy(),
        ResolvedProxy::Url(url) => {
            let proxy = reqwest::Proxy::all(url).map_err(|_| EngineError::Request)?;
            builder = builder.proxy(proxy);
        }
    }
    builder.build().map_err(|_| EngineError::Request)
}

fn apply_source_headers(
    mut request: RequestBuilder,
    source: &DownloadSource,
) -> Result<RequestBuilder, EngineError> {
    for header in &source.headers {
        let name = HeaderName::from_bytes(header.name.as_bytes())
            .map_err(|_| EngineError::InvalidHeader(header.name.clone()))?;
        if name == RANGE
            || name == IF_RANGE
            || name == CONTENT_LENGTH
            || name == CONTENT_RANGE
            || name == COOKIE
            || matches!(
                name.as_str(),
                "accept-encoding"
                    | "connection"
                    | "host"
                    | "te"
                    | "trailer"
                    | "transfer-encoding"
                    | "upgrade"
            )
        {
            return Err(EngineError::InvalidHeader(header.name.clone()));
        }
        let value = HeaderValue::from_str(&header.value)
            .map_err(|_| EngineError::InvalidHeader(header.name.clone()))?;
        request = request.header(name, value);
    }
    if !source.cookies.is_empty() {
        let value = source
            .cookies
            .iter()
            .map(|cookie| format!("{}={}", cookie.name, cookie.value))
            .collect::<Vec<_>>()
            .join("; ");
        request = request.header(
            COOKIE,
            HeaderValue::from_str(&value)
                .map_err(|_| EngineError::InvalidHeader("Cookie".to_owned()))?,
        );
    }
    Ok(request)
}

fn apply_if_range(
    request: RequestBuilder,
    validator: &SourceValidator,
) -> Result<RequestBuilder, EngineError> {
    let value = match validator {
        SourceValidator::None => return Ok(request),
        SourceValidator::Etag { value } | SourceValidator::LastModified { value } => value,
    };
    let header = HeaderValue::from_str(value)
        .map_err(|_| EngineError::InvalidHeader("If-Range".to_owned()))?;
    Ok(request.header(IF_RANGE, header))
}

fn response_validator(headers: &reqwest::header::HeaderMap) -> SourceValidator {
    if let Some(value) = headers.get(ETAG).and_then(|value| value.to_str().ok()) {
        if !value.trim_start().starts_with("W/") {
            return SourceValidator::Etag {
                value: value.to_owned(),
            };
        }
    }
    if let Some(value) = headers
        .get(LAST_MODIFIED)
        .and_then(|value| value.to_str().ok())
    {
        return SourceValidator::LastModified {
            value: value.to_owned(),
        };
    }
    SourceValidator::None
}

fn accepts_ranges(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get(ACCEPT_RANGES)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("bytes"))
}

fn resume_support(accepts_ranges: bool) -> ResumeSupport {
    if !accepts_ranges {
        ResumeSupport::Unsupported {
            reason: "El servidor no acepta solicitudes por rango".to_owned(),
        }
    } else {
        ResumeSupport::Supported
    }
}

fn content_disposition_file_name(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let value = headers.get(CONTENT_DISPOSITION)?.to_str().ok()?;
    let mut regular = None;
    for parameter in split_header_parameters(value).into_iter().skip(1) {
        let Some((name, raw_value)) = parameter.split_once('=') else {
            continue;
        };
        let name = name.trim();
        let raw_value = unquote_header_value(raw_value.trim());
        if name.eq_ignore_ascii_case("filename*") {
            if let Some((charset, encoded)) = raw_value.split_once("''") {
                if charset.eq_ignore_ascii_case("UTF-8") {
                    if let Some(decoded) = percent_decode_utf8(encoded) {
                        return Some(decoded);
                    }
                }
            }
        } else if name.eq_ignore_ascii_case("filename") && regular.is_none() {
            regular = Some(raw_value);
        }
    }
    regular.filter(|value| !value.is_empty())
}

fn split_header_parameters(value: &str) -> Vec<String> {
    let mut parameters = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            current.push(character);
            escaped = false;
        } else if character == '\\' && quoted {
            current.push(character);
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
            current.push(character);
        } else if character == ';' && !quoted {
            parameters.push(current.trim().to_owned());
            current.clear();
        } else {
            current.push(character);
        }
    }
    parameters.push(current.trim().to_owned());
    parameters
}

fn unquote_header_value(value: &str) -> String {
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value);
    let mut output = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            output.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            output.push(character);
        }
    }
    if escaped {
        output.push('\\');
    }
    output
}

fn percent_decode_utf8(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            let text = std::str::from_utf8(hex).ok()?;
            decoded.push(u8::from_str_radix(text, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn file_name_from_response_url(url: &reqwest::Url) -> Option<String> {
    let segment = url.path_segments()?.rfind(|segment| !segment.is_empty())?;
    percent_decode_utf8(segment).filter(|value| !value.is_empty())
}

fn ensure_same_source(
    downloaded: u64,
    previous: &SourceValidator,
    current: &SourceValidator,
) -> Result<(), EngineError> {
    if downloaded > 0
        && !matches!(previous, SourceValidator::None)
        && !matches!(current, SourceValidator::None)
        && previous != current
    {
        return Err(EngineError::SourceChanged);
    }
    Ok(())
}

fn response_size(headers: &reqwest::header::HeaderMap, resumed: u64) -> TransferSize {
    if resumed > 0 {
        if let Some(total_bytes) = content_range_total(headers) {
            return TransferSize::Known { total_bytes };
        }
    }
    headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(TransferSize::Unknown, |remaining| TransferSize::Known {
            total_bytes: resumed + remaining,
        })
}

fn ensure_same_size(
    downloaded: u64,
    previous: &TransferSize,
    current: &TransferSize,
) -> Result<(), EngineError> {
    if downloaded > 0 {
        match (previous, current) {
            (
                TransferSize::Known {
                    total_bytes: previous,
                },
                TransferSize::Known {
                    total_bytes: current,
                },
            ) if previous == current => {}
            (TransferSize::Known { .. }, _) => return Err(EngineError::SourceChanged),
            _ => {}
        }
    }
    Ok(())
}

fn validate_content_range(
    headers: &reqwest::header::HeaderMap,
    expected_start: u64,
    expected_end: Option<u64>,
    expected_total: Option<u64>,
) -> Result<(), EngineError> {
    let value = headers
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .ok_or(EngineError::InvalidContentRange)?;
    let bounds = value
        .strip_prefix("bytes ")
        .and_then(|value| value.split('/').next())
        .and_then(|value| value.split_once('-'))
        .and_then(|(start, end)| Some((start.parse::<u64>().ok()?, end.parse::<u64>().ok()?)))
        .ok_or(EngineError::InvalidContentRange)?;
    if bounds.1 < bounds.0
        || bounds.0 != expected_start
        || expected_end.is_some_and(|end| end != bounds.1)
    {
        return Err(EngineError::InvalidContentRange);
    }
    if expected_total.is_some_and(|total| content_range_total(headers) != Some(total)) {
        return Err(EngineError::InvalidContentRange);
    }
    if headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length != bounds.1 - bounds.0 + 1)
    {
        return Err(EngineError::InvalidContentRange);
    }
    Ok(())
}

fn content_range_total(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split('/').nth(1))
        .and_then(|value| value.parse::<u64>().ok())
}

fn split_ranges(total: u64, requested_threads: u8) -> Vec<(u64, u64)> {
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

async fn merge_segments(
    destination: &Path,
    directory: &Path,
    file_name: &str,
    count: usize,
    cancellation: &CancellationToken,
) -> Result<(), EngineError> {
    let output = File::create(destination).await.map_err(EngineError::File)?;
    let mut output = BufWriter::new(output);
    let copied = async {
        for index in 0..count {
            let path = segment_path(directory, file_name, index);
            let mut segment = File::open(&path).await.map_err(EngineError::File)?;
            tokio::select! {
                _ = cancellation.cancelled() => return Err(EngineError::Cancelled),
                result = tokio::io::copy(&mut segment, &mut output) => {
                    result.map_err(EngineError::File)?;
                }
            }
        }
        output.flush().await.map_err(EngineError::File)?;
        output.get_ref().sync_all().await.map_err(EngineError::File)
    }
    .await;
    drop(output);
    if let Err(error) = copied {
        let _ = fs::remove_file(destination).await;
        return Err(error);
    }
    if cancellation.is_cancelled() {
        let _ = fs::remove_file(destination).await;
        return Err(EngineError::Cancelled);
    }
    for index in 0..count {
        fs::remove_file(segment_path(directory, file_name, index))
            .await
            .map_err(EngineError::File)?;
    }
    Ok(())
}

async fn open_partial(path: &Path, append: bool) -> Result<BufWriter<File>, EngineError> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(path)
        .await
        .map_err(EngineError::File)?;
    Ok(BufWriter::new(file))
}

async fn file_len(path: &Path) -> Result<u64, EngineError> {
    match fs::metadata(path).await {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(EngineError::File(error)),
    }
}

async fn has_segment_partials(
    directory: &Path,
    file_name: &str,
    threads: u8,
) -> Result<bool, EngineError> {
    for index in 0..usize::from(threads) {
        if file_len(&segment_path(directory, file_name, index)).await? > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn remove_segment_partials(
    directory: &Path,
    file_name: &str,
    threads: u8,
) -> Result<(), EngineError> {
    for index in 0..usize::from(threads) {
        let path = segment_path(directory, file_name, index);
        match fs::remove_file(path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(EngineError::File(error)),
        }
    }
    Ok(())
}

fn segmented_failure_allows_single_stream(error: &EngineError) -> bool {
    matches!(
        error,
        EngineError::ResumeRejected | EngineError::InvalidContentRange
    )
}

async fn ensure_destination_available(path: &Path) -> Result<(), EngineError> {
    if fs::try_exists(path).await.map_err(EngineError::File)? {
        return Err(EngineError::DestinationExists);
    }
    Ok(())
}

async fn finalize_partial(partial: &Path, destination: &Path) -> Result<(), EngineError> {
    match fs::hard_link(partial, destination).await {
        Ok(()) => {
            let _ = fs::remove_file(partial).await;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(EngineError::DestinationExists)
        }
        Err(_) => copy_without_clobber(partial, destination).await,
    }
}

async fn copy_without_clobber(partial: &Path, destination: &Path) -> Result<(), EngineError> {
    let marker = finalization_marker(destination);
    fs::write(&marker, b"fluxor-finalizing")
        .await
        .map_err(EngineError::File)?;
    let mut source = File::open(partial).await.map_err(EngineError::File)?;
    let mut output = match OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .await
    {
        Ok(file) => file,
        Err(error) => {
            let _ = fs::remove_file(&marker).await;
            return if error.kind() == std::io::ErrorKind::AlreadyExists {
                Err(EngineError::DestinationExists)
            } else {
                Err(EngineError::File(error))
            };
        }
    };
    if let Err(error) = tokio::io::copy(&mut source, &mut output).await {
        let _ = fs::remove_file(destination).await;
        let _ = fs::remove_file(&marker).await;
        return Err(EngineError::File(error));
    }
    output.sync_all().await.map_err(EngineError::File)?;
    fs::remove_file(&marker).await.map_err(EngineError::File)?;
    fs::remove_file(partial).await.map_err(EngineError::File)
}

async fn recover_interrupted_finalization(
    directory: &Path,
    file_name: &str,
) -> Result<(), EngineError> {
    let destination = directory.join(file_name);
    let marker = finalization_marker(&destination);
    if fs::try_exists(&marker).await.map_err(EngineError::File)? {
        match fs::remove_file(&destination).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(EngineError::File(error)),
        }
        fs::remove_file(marker).await.map_err(EngineError::File)?;
    }
    Ok(())
}

fn finalization_marker(destination: &Path) -> PathBuf {
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    destination.with_file_name(format!(".{file_name}.fluxor.finalizing"))
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

fn verify_complete(downloaded: u64, size: &TransferSize) -> Result<(), EngineError> {
    if let TransferSize::Known { total_bytes } = size {
        if downloaded != *total_bytes {
            return Err(EngineError::Request);
        }
    }
    Ok(())
}

fn partial_path(directory: &Path, file_name: &str) -> PathBuf {
    directory.join(format!(".{file_name}.fluxor.part"))
}

fn segment_path(directory: &Path, file_name: &str, index: usize) -> PathBuf {
    directory.join(format!(".{file_name}.fluxor.part.{index}"))
}

fn send_progress(
    sender: &watch::Sender<Option<EngineProgress>>,
    downloaded_bytes: u64,
    size: &TransferSize,
    validator: &SourceValidator,
    resume: &ResumeSupport,
    speed_bytes: u64,
    telemetry: TransferTelemetry,
) {
    sender.send_replace(Some(EngineProgress {
        downloaded_bytes,
        size: size.clone(),
        validator: validator.clone(),
        resume: resume.clone(),
        speed_bytes,
        telemetry,
    }));
}

fn single_telemetry(
    phase: TransferPhase,
    reason: Option<String>,
    size: &TransferSize,
    downloaded_bytes: u64,
    speed_bytes: u64,
    state: SegmentState,
    last_activity_at: Option<DateTime<Utc>>,
) -> TransferTelemetry {
    let end_byte = match size {
        TransferSize::Known { total_bytes } if *total_bytes > 0 => Some(total_bytes - 1),
        _ => None,
    };
    TransferTelemetry {
        phase,
        mode: TransferMode::Single { reason },
        segments: vec![SegmentProgress {
            index: 0,
            start_byte: 0,
            end_byte,
            downloaded_bytes,
            speed_bytes,
            state,
            last_activity_at,
            error: None,
        }],
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

fn segments_downloaded(segments: &[SegmentProgress]) -> u64 {
    segments
        .iter()
        .map(|segment| segment.downloaded_bytes)
        .sum()
}

fn segments_speed(segments: &[SegmentProgress]) -> u64 {
    segments.iter().map(|segment| segment.speed_bytes).sum()
}

fn sample_segments(
    runtimes: &[Arc<SegmentRuntime>],
    checkpoints: &mut [u64],
    elapsed: Duration,
) -> Vec<SegmentProgress> {
    let speeds = runtimes
        .iter()
        .enumerate()
        .map(|(index, runtime)| {
            let current = runtime.downloaded_bytes.load(Ordering::Relaxed);
            let speed = bytes_per_second(
                current.saturating_sub(checkpoints.get(index).copied().unwrap_or(0)),
                elapsed,
            );
            if let Some(checkpoint) = checkpoints.get_mut(index) {
                *checkpoint = current;
            }
            speed
        })
        .collect::<Vec<_>>();
    snapshot_segments(runtimes, &speeds)
}

fn bytes_per_second(bytes: u64, elapsed: Duration) -> u64 {
    if elapsed.is_zero() {
        return 0;
    }
    (bytes as f64 / elapsed.as_secs_f64()) as u64
}

fn transfer_duration(bytes: usize, bytes_per_second: u64) -> Duration {
    Duration::from_secs_f64(bytes as f64 / bytes_per_second as f64)
}

#[cfg(test)]
mod tests {
    use reqwest::header::{
        HeaderMap, HeaderValue, ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_RANGE, ETAG,
        LAST_MODIFIED,
    };

    use super::{
        content_disposition_file_name, content_range_total, ensure_same_source,
        file_name_from_response_url, merge_segments, response_validator, resume_support,
        segmented_failure_allows_single_stream, segments_downloaded, segments_speed,
        single_stream_reason, split_ranges, transfer_duration, validate_content_range, EngineError,
        ProbeResult, ResumeSupport, SegmentRuntime, SegmentState, SourceValidator, TransferSize,
        MIN_SEGMENT_SIZE, SEGMENT_DOWNLOADING,
    };

    #[test]
    fn split_ranges_covers_the_file_without_gaps() {
        let total = MIN_SEGMENT_SIZE * 5 + 17;
        let ranges = split_ranges(total, 4);

        assert_eq!(ranges.first().map(|range| range.0), Some(0));
        assert_eq!(ranges.last().map(|range| range.1), Some(total - 1));
        for pair in ranges.windows(2) {
            assert_eq!(pair[0].1 + 1, pair[1].0);
        }
        assert!(ranges.len() <= 4);
    }

    #[test]
    fn segment_runtime_reports_only_its_own_range_and_progress() {
        let runtime = SegmentRuntime::new(2, 200, 299, 20);
        runtime.set_state(SEGMENT_DOWNLOADING);
        runtime.mark_activity(30);

        let segment = runtime.snapshot(4_096);

        assert_eq!(segment.index, 2);
        assert_eq!((segment.start_byte, segment.end_byte), (200, Some(299)));
        assert_eq!(segment.downloaded_bytes, 50);
        assert_eq!(segment.speed_bytes, 4_096);
        assert!(matches!(segment.state, SegmentState::Downloading));
        assert!(segment.last_activity_at.is_some());
        assert_eq!(segments_downloaded(std::slice::from_ref(&segment)), 50);
        assert_eq!(segments_speed(std::slice::from_ref(&segment)), 4_096);
    }

    #[test]
    fn single_stream_reason_explains_why_segments_are_not_used() {
        assert!(single_stream_reason(None, 8).contains("confirmar"));
        let small = ProbeResult {
            size: TransferSize::Known {
                total_bytes: MIN_SEGMENT_SIZE - 1,
            },
            validator: SourceValidator::Etag {
                value: "\"safe\"".to_owned(),
            },
            accepts_ranges: true,
        };
        assert!(single_stream_reason(Some(&small), 8).contains("pequeño"));
    }

    #[tokio::test]
    async fn cancelled_merge_keeps_segments_available_for_resume() {
        let directory = std::env::temp_dir().join(format!("fluxor-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&directory).await.unwrap();
        let file_name = "archive.zip";
        let segment = super::segment_path(&directory, file_name, 0);
        tokio::fs::write(&segment, b"partial").await.unwrap();
        let destination = super::partial_path(&directory, file_name);
        let cancellation = tokio_util::sync::CancellationToken::new();
        cancellation.cancel();

        let result = merge_segments(&destination, &directory, file_name, 1, &cancellation).await;

        assert!(matches!(result, Err(EngineError::Cancelled)));
        assert!(tokio::fs::try_exists(segment).await.unwrap());
        assert!(!tokio::fs::try_exists(destination).await.unwrap());
        tokio::fs::remove_dir_all(directory).await.unwrap();
    }

    #[test]
    fn content_range_must_match_both_requested_bounds() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_RANGE,
            HeaderValue::from_static("bytes 100-199/1000"),
        );

        assert!(validate_content_range(&headers, 100, Some(199), Some(1000)).is_ok());
        assert!(matches!(
            validate_content_range(&headers, 100, Some(299), Some(1000)),
            Err(EngineError::InvalidContentRange)
        ));
        assert!(matches!(
            validate_content_range(&headers, 100, Some(199), Some(2000)),
            Err(EngineError::InvalidContentRange)
        ));
        assert_eq!(content_range_total(&headers), Some(1000));
    }

    #[test]
    fn malformed_content_range_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_RANGE, HeaderValue::from_static("items 0-10/20"));

        assert!(matches!(
            validate_content_range(&headers, 0, None, None),
            Err(EngineError::InvalidContentRange)
        ));
    }

    #[test]
    fn partial_without_a_durable_validator_can_resume_by_exact_range() {
        assert!(ensure_same_source(100, &SourceValidator::None, &SourceValidator::None).is_ok());
    }

    #[test]
    fn conflicting_durable_validators_are_rejected() {
        let previous = SourceValidator::Etag {
            value: "\"old\"".to_owned(),
        };
        let current = SourceValidator::Etag {
            value: "\"new\"".to_owned(),
        };

        assert!(matches!(
            ensure_same_source(100, &previous, &current),
            Err(EngineError::SourceChanged)
        ));
    }

    #[test]
    fn content_disposition_prefers_utf8_file_name() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static(
                "attachment; filename=report.pdf; filename*=UTF-8''informe%20final.pdf",
            ),
        );

        assert_eq!(
            content_disposition_file_name(&headers).as_deref(),
            Some("informe final.pdf")
        );
    }

    #[test]
    fn content_disposition_handles_quoted_semicolons() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename=\"report; final.pdf\""),
        );

        assert_eq!(
            content_disposition_file_name(&headers).as_deref(),
            Some("report; final.pdf")
        );
    }

    #[test]
    fn final_response_url_is_percent_decoded() {
        let url = reqwest::Url::parse("https://cdn.example.com/files/video%20final.mp4").unwrap();

        assert_eq!(
            file_name_from_response_url(&url).as_deref(),
            Some("video final.mp4")
        );
    }

    #[test]
    fn range_and_connection_failures_can_degrade_to_one_stream() {
        assert!(segmented_failure_allows_single_stream(
            &EngineError::ResumeRejected
        ));
        assert!(!segmented_failure_allows_single_stream(
            &EngineError::Request
        ));
        assert!(!segmented_failure_allows_single_stream(
            &EngineError::SourceChanged
        ));
    }

    #[test]
    fn rejected_resume_requires_restart() {
        assert!(!EngineError::ResumeRejected.recoverable());
        assert!(EngineError::Request.recoverable());
    }

    #[test]
    fn resume_support_follows_range_capability() {
        assert!(matches!(resume_support(true), ResumeSupport::Supported));
        assert!(matches!(
            resume_support(false),
            ResumeSupport::Unsupported { .. }
        ));
    }

    #[test]
    fn weak_etag_uses_last_modified_instead() {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
        headers.insert(ETAG, HeaderValue::from_static("W/\"weak\""));
        headers.insert(
            LAST_MODIFIED,
            HeaderValue::from_static("Tue, 11 Aug 2026 00:00:00 GMT"),
        );

        assert!(matches!(
            response_validator(&headers),
            SourceValidator::LastModified { .. }
        ));
    }

    #[test]
    fn bandwidth_duration_uses_the_aggregate_byte_rate() {
        assert_eq!(
            transfer_duration(512 * 1024, 1024 * 1024),
            std::time::Duration::from_millis(500)
        );
    }
}
