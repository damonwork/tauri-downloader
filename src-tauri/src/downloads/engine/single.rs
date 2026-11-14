use std::time::Instant;

use chrono::{DateTime, Utc};
use reqwest::{header::RANGE, Client, StatusCode};
use tokio::{io::AsyncWriteExt, sync::watch, time};
use tokio_util::sync::CancellationToken;

use super::http::{
    apply_if_range, apply_source_headers, content_range_total, ensure_same_size,
    ensure_same_source, response_size, response_validator, validate_content_range,
};
use super::rate::{BandwidthLimiter, TransferRateEstimator};
use super::storage::{
    ensure_destination_available, file_len, finalize_partial, open_partial, partial_path,
    remove_segment_partials, verify_complete,
};
use super::{
    send_progress, EngineError, EngineInput, EngineOutput, EngineProgress, PROGRESS_INTERVAL,
};
use crate::downloads::model::{
    ResumeSupport, SegmentProgress, SegmentState, SourceValidator, TransferMode, TransferPhase,
    TransferSize, TransferTelemetry,
};

pub(super) async fn fallback_to_single(
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

pub(super) async fn run_single(
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

    // A crash/pause after the final write can leave a complete partial. Sending
    // Range: bytes=<total>- would produce 416 and wrongly demand a restart.
    if matches!(&input.item.transfer.size, TransferSize::Known { total_bytes } if *total_bytes > 0 && resume_at == *total_bytes)
    {
        let mut output = open_partial(&partial_path, true).await?;
        output.flush().await.map_err(EngineError::File)?;
        output.sync_all().await.map_err(EngineError::File)?;
        drop(output);
        if cancellation.is_cancelled() {
            return Err(EngineError::Cancelled);
        }
        finalize_partial(&partial_path, &final_path).await?;
        let telemetry = single_telemetry(
            TransferPhase::Finalizing,
            reason,
            &input.item.transfer.size,
            resume_at,
            0,
            SegmentState::Completed,
            None,
        );
        send_progress(
            &progress,
            resume_at,
            &input.item.transfer.size,
            &input.item.transfer.validator,
            &input.item.transfer.resume,
            0,
            telemetry.clone(),
        );
        return Ok(EngineOutput {
            downloaded_bytes: resume_at,
            size: input.item.transfer.size,
            validator: input.item.transfer.validator,
            resume: input.item.transfer.resume,
            telemetry,
        });
    }

    let mut request = apply_source_headers(client.get(&input.item.source.url), &input.item.source)?;
    // The transfer itself confirms range support, without an extra GET.
    request = request.header(RANGE, format!("bytes={resume_at}-"));
    if resume_at > 0 {
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
    // Some origins reject even an initial Range request. A fresh automatic
    // transfer may safely retry without it; an existing partial must never do so.
    if resume_at == 0
        && !input.item.source.force_single_stream
        && matches!(
            response.status(),
            StatusCode::BAD_REQUEST | StatusCode::RANGE_NOT_SATISFIABLE
        )
    {
        let request = apply_source_headers(client.get(&input.item.source.url), &input.item.source)?;
        response = tokio::select! {
            _ = cancellation.cancelled() => return Err(EngineError::Cancelled),
            response = request.send() => response.map_err(|_| EngineError::Request)?,
        };
    }
    if !response.status().is_success() {
        return Err(EngineError::HttpStatus(response.status().as_u16()));
    }
    if resume_at > 0 && response.status() != StatusCode::PARTIAL_CONTENT {
        return Err(EngineError::ResumeRejected);
    }
    if response.status() == StatusCode::PARTIAL_CONTENT {
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
    let resume = if response.status() == StatusCode::PARTIAL_CONTENT {
        ResumeSupport::Supported
    } else {
        ResumeSupport::Unsupported {
            reason: "El servidor ignoró la solicitud por rango".to_owned(),
        }
    };
    let size = response_size(response.headers(), resume_at);
    ensure_same_size(resume_at, &input.item.transfer.size, &size)?;
    let mut output = open_partial(&partial_path, resume_at > 0).await?;
    let limiter = BandwidthLimiter::shared(input.speed_limit.clone());
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
    let transfer = async {
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
        if matches!(&size, TransferSize::Known { total_bytes } if downloaded.saturating_add(chunk.len() as u64) > *total_bytes)
        {
            return Err(EngineError::InvalidContentRange);
        }
        let mut offset = 0;
        while offset < chunk.len() {
            let limit = input.speed_limit.load(std::sync::atomic::Ordering::Relaxed);
            let block_size = if limit == 0 {
                chunk.len()
            } else {
                (limit / 10).clamp(1, 65_536) as usize
            };
            let end = (offset + block_size).min(chunk.len());
            let block = &chunk[offset..end];
            let acquire = limiter.acquire(block.len(), &cancellation);
            tokio::pin!(acquire);
            loop {
                tokio::select! {
                    result = &mut acquire => { result?; break; }
                    _ = interval.tick() => {
                        let speed = rate.sample(downloaded, Instant::now());
                        send_progress(&progress, downloaded, &size, &validator, &resume, speed,
                            single_telemetry(TransferPhase::Transferring, reason.clone(), &size,
                                downloaded, speed, SegmentState::Downloading, last_activity_at));
                    }
                }
            }
            output.write_all(block).await.map_err(EngineError::File)?;
            downloaded += block.len() as u64;
            last_activity_at = Some(Utc::now());
            offset = end;
        }
    }
        Ok::<(), EngineError>(())
    }.await;
    // Tokio file writes may still be queued when write_all completes. Drain
    // them before returning an interruption so a retry observes stable length.
    output.flush().await.map_err(EngineError::File)?;
    output.sync_data().await.map_err(EngineError::File)?;
    transfer?;

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
    output.sync_all().await.map_err(EngineError::File)?;
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
