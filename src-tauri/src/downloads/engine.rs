use std::{path::PathBuf, time::Duration};

use thiserror::Error;
use tokio::{fs, sync::watch};
use tokio_util::sync::CancellationToken;

use super::model::{
    DownloadItem, DownloadSource, ResumeSupport, SourceValidator, TransferPhase, TransferSize,
    TransferTelemetry,
};

mod http;
mod rate;
mod segments;
mod single;
mod storage;

use http::{build_client, probe_source, ProbeResult};
use segments::{run_segmented, SegmentMetadata};
use single::{fallback_to_single, run_single};
use storage::{file_len, has_segment_partials, partial_path, recover_interrupted_finalization};

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
    #[error("El servidor rechazó temporalmente un segmento: HTTP {0}")]
    SegmentHttpStatus(u16),
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
    #[error("La transferencia segmentada se interrumpió; reintenta para continuar")]
    SegmentInterrupted,
}

impl EngineError {
    pub fn recoverable(&self) -> bool {
        matches!(
            self,
            Self::Request
                | Self::HttpStatus(_)
                | Self::SegmentHttpStatus(_)
                | Self::SegmentTask
                | Self::SegmentInterrupted
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
                    Err(error)
                        if segmented_failure_allows_single_stream(&error, has_segment_partials) =>
                    {
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
            return if probe.is_some() {
                Err(EngineError::ResumeRejected)
            } else {
                Err(EngineError::SegmentInterrupted)
            };
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

fn segmented_failure_allows_single_stream(error: &EngineError, has_segment_partials: bool) -> bool {
    !has_segment_partials
        && matches!(
            error,
            EngineError::Request
                | EngineError::HttpStatus(_)
                | EngineError::SegmentHttpStatus(_)
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

#[cfg(test)]
mod tests;
