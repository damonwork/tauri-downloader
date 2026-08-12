use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use reqwest::{header::RANGE, Client, StatusCode};
use thiserror::Error;
use tokio::{fs, io::AsyncWriteExt, sync::watch, time};
use tokio_util::sync::CancellationToken;

use super::model::{
    DownloadItem, DownloadSource, ResumeSupport, SegmentProgress, SegmentState, SourceValidator,
    TransferMode, TransferPhase, TransferSize, TransferTelemetry,
};

mod http;
mod rate;
mod segments;
mod storage;

use http::{
    accepts_ranges, apply_if_range, apply_source_headers, build_client, content_range_total,
    ensure_same_size, ensure_same_source, probe_source, response_size, response_validator,
    resume_support, validate_content_range, ProbeResult,
};
use rate::{BandwidthLimiter, TransferRateEstimator};
use segments::{run_segmented, SegmentMetadata};
use storage::{
    ensure_destination_available, file_len, finalize_partial, has_segment_partials, open_partial,
    partial_path, recover_interrupted_finalization, remove_segment_partials, verify_complete,
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
        http::detect_file_name(source, proxy).await
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
            return fallback_to_single(
                input,
                client,
                cancellation,
                progress,
                Some("Se encontró un parcial creado como flujo único".to_owned()),
                false,
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
        let probe = probe_source(&client, &input.item.source, &cancellation).await?;

        if let Some(probe) = probe.clone() {
            if supports_segmented_transfer(&probe, input.item.threads) {
                let segmented = run_segmented(
                    input.clone(),
                    client.clone(),
                    probe,
                    has_segment_partials,
                    cancellation.clone(),
                    progress.clone(),
                )
                .await;
                match segmented {
                    Ok(output) => return Ok(output),
                    Err(error) if segmented_failure_allows_single_stream(&error) => {
                        return fallback_to_single(
                            input,
                            client,
                            cancellation,
                            progress,
                            Some(
                                "La transferencia segmentada no pudo continuar; se usa un flujo"
                                    .to_owned(),
                            ),
                            true,
                        )
                        .await;
                    }
                    Err(error) => return Err(error),
                }
            }
        }

        if has_segment_partials {
            return fallback_to_single(
                input,
                client,
                cancellation,
                progress,
                Some("Los segmentos guardados ya no pueden continuar; se usa un flujo".to_owned()),
                true,
            )
            .await;
        }

        let reason = single_stream_reason(probe.as_ref(), input.item.threads);
        run_single(input, client, cancellation, progress, Some(reason)).await
    }
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
    match &probe.size {
        TransferSize::Unknown => "El servidor no informó el tamaño del archivo".to_owned(),
        TransferSize::Known { total_bytes } if *total_bytes < MIN_SEGMENT_SIZE => {
            "El archivo es demasiado pequeño para dividirlo".to_owned()
        }
        _ => "Se seleccionó un único flujo".to_owned(),
    }
}

fn supports_segmented_transfer(probe: &ProbeResult, requested_threads: u8) -> bool {
    probe.accepts_ranges
        && requested_threads > 1
        && matches!(&probe.size, TransferSize::Known { total_bytes } if *total_bytes >= MIN_SEGMENT_SIZE)
}

fn ensure_segment_partials_compatible(
    previous_size: &TransferSize,
    current_size: &TransferSize,
    previous_validator: &SourceValidator,
    current_validator: &SourceValidator,
) -> Result<(), EngineError> {
    let validators_match = matches!(
        (previous_validator, current_validator),
        (SourceValidator::None, SourceValidator::None)
    ) || previous_validator == current_validator;
    if !validators_match {
        return Err(EngineError::SourceChanged);
    }
    match (previous_size, current_size) {
        (
            TransferSize::Known {
                total_bytes: previous,
            },
            TransferSize::Known {
                total_bytes: current,
            },
        ) if previous == current => Ok(()),
        _ => Err(EngineError::SourceChanged),
    }
}

async fn fallback_to_single(
    input: EngineInput,
    client: Client,
    cancellation: CancellationToken,
    progress: watch::Sender<Option<EngineProgress>>,
    reason: Option<String>,
    discard_segments: bool,
) -> Result<EngineOutput, EngineError> {
    if cancellation.is_cancelled() {
        return Err(EngineError::Cancelled);
    }
    let final_path = input.destination_dir.join(&input.item.file_name);
    ensure_destination_available(&final_path).await?;
    if discard_segments {
        remove_segment_partials(
            &input.destination_dir,
            &input.item.file_name,
            input.item.threads,
        )
        .await?;
    }
    let directory = input.destination_dir.clone();
    let file_name = input.item.file_name.clone();
    let threads = input.item.threads;
    let output = run_single(input, client, cancellation, progress, reason).await?;
    if !discard_segments {
        let _ = remove_segment_partials(&directory, &file_name, threads).await;
    }
    Ok(output)
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
    let mut rate = TransferRateEstimator::new(downloaded, Instant::now());
    let mut last_activity_at = None;
    let mut interval = time::interval(PROGRESS_INTERVAL);
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
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
                let speed = rate.sample(downloaded, Instant::now());
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

fn segmented_failure_allows_single_stream(error: &EngineError) -> bool {
    matches!(
        error,
        EngineError::Request
            | EngineError::HttpStatus(_)
            | EngineError::ResumeRejected
            | EngineError::SourceChanged
            | EngineError::InvalidContentRange
    )
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

#[allow(clippy::too_many_arguments)]
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

#[cfg(test)]
mod tests;
