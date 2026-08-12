use std::path::{Path, PathBuf};

use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncWriteExt, BufWriter},
};
use tokio_util::sync::CancellationToken;

use super::{EngineError, SegmentMetadata};
use crate::downloads::model::TransferSize;

pub(super) async fn merge_segments(
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

pub(super) async fn open_partial(
    path: &Path,
    append: bool,
) -> Result<BufWriter<File>, EngineError> {
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

pub(super) async fn file_len(path: &Path) -> Result<u64, EngineError> {
    match fs::metadata(path).await {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(EngineError::File(error)),
    }
}

pub(super) async fn read_segment_metadata(
    path: &Path,
) -> Result<Option<SegmentMetadata>, EngineError> {
    let bytes = match fs::read(path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(EngineError::File(error)),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| EngineError::SourceChanged)
}

pub(super) fn require_segment_metadata(
    has_segment_partials: bool,
    metadata: Option<&SegmentMetadata>,
) -> Result<(), EngineError> {
    if has_segment_partials && metadata.is_none() {
        return Err(EngineError::SourceChanged);
    }
    Ok(())
}

pub(super) async fn persist_segment_metadata(
    path: &Path,
    metadata: &SegmentMetadata,
) -> Result<(), EngineError> {
    let bytes = serde_json::to_vec(metadata).map_err(|error| {
        EngineError::File(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    })?;
    let mut file = File::create(path).await.map_err(EngineError::File)?;
    file.write_all(&bytes).await.map_err(EngineError::File)?;
    file.flush().await.map_err(EngineError::File)?;
    file.sync_all().await.map_err(EngineError::File)
}

pub(super) async fn remove_segment_metadata(path: &Path) -> Result<(), EngineError> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(EngineError::File(error)),
    }
}

pub(super) async fn has_segment_partials(
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

pub(super) async fn remove_segment_partials(
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
    remove_segment_metadata(&segment_metadata_path(directory, file_name)).await
}

pub(super) async fn ensure_destination_available(path: &Path) -> Result<(), EngineError> {
    if fs::try_exists(path).await.map_err(EngineError::File)? {
        return Err(EngineError::DestinationExists);
    }
    Ok(())
}

pub(super) async fn finalize_partial(
    partial: &Path,
    destination: &Path,
) -> Result<(), EngineError> {
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

pub(super) async fn recover_interrupted_finalization(
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

pub(super) fn verify_complete(downloaded: u64, size: &TransferSize) -> Result<(), EngineError> {
    if let TransferSize::Known { total_bytes } = size {
        if downloaded != *total_bytes {
            return Err(EngineError::Request);
        }
    }
    Ok(())
}

pub(super) fn partial_path(directory: &Path, file_name: &str) -> PathBuf {
    directory.join(format!(".{file_name}.fluxor.part"))
}

pub(super) fn segment_metadata_path(directory: &Path, file_name: &str) -> PathBuf {
    directory.join(format!(".{file_name}.fluxor.segments.json"))
}

pub(super) fn segment_path(directory: &Path, file_name: &str, index: usize) -> PathBuf {
    directory.join(format!(".{file_name}.fluxor.part.{index}"))
}
