use std::path::Path;

use tokio::fs;

use super::{
    AppError, DownloadItem, DownloadState, SegmentState, TransferPhase, MAX_THREADS_PER_DOWNLOAD,
};

pub(super) async fn remove_partial_files(
    directory: &Path,
    file_name: &str,
) -> Result<(), AppError> {
    let partial = directory.join(format!(".{file_name}.fluxor.part"));
    remove_if_exists(&partial).await?;
    for index in 0..usize::from(MAX_THREADS_PER_DOWNLOAD) {
        let segment = directory.join(format!(".{file_name}.fluxor.part.{index}"));
        remove_if_exists(&segment).await?;
    }
    let metadata = directory.join(format!(".{file_name}.fluxor.segments.json"));
    remove_if_exists(&metadata).await
}

pub(super) async fn remove_if_exists(path: &Path) -> Result<(), AppError> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(super) async fn stored_file_len(path: &Path) -> Result<u64, AppError> {
    match fs::metadata(path).await {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn reset_live_telemetry(item: &mut DownloadItem) {
    item.telemetry.phase = TransferPhase::Idle;
    let resting_state = if matches!(&item.state, DownloadState::Paused) {
        SegmentState::Paused
    } else {
        SegmentState::Pending
    };
    let preserve_failure = matches!(&item.state, DownloadState::Failed { .. });
    for segment in &mut item.telemetry.segments {
        segment.speed_bytes = 0;
        if !(matches!(segment.state, SegmentState::Completed)
            || preserve_failure && matches!(segment.state, SegmentState::Failed))
        {
            segment.state = resting_state.clone();
        }
        if !preserve_failure {
            segment.error = None;
        }
    }
}

pub(super) fn pause_telemetry(item: &mut DownloadItem) {
    item.telemetry.phase = TransferPhase::Idle;
    for segment in &mut item.telemetry.segments {
        segment.speed_bytes = 0;
        if !matches!(segment.state, SegmentState::Completed) {
            segment.state = SegmentState::Paused;
        }
    }
}

#[cfg(unix)]
pub(super) async fn set_private_permissions(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
    Ok(())
}

#[cfg(not(unix))]
pub(super) async fn set_private_permissions(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

#[cfg(unix)]
pub(super) async fn sync_parent_directory(path: &Path) -> Result<(), AppError> {
    let parent = path.parent().ok_or(AppError::AppDirectory)?;
    let directory = fs::File::open(parent).await?;
    directory.sync_all().await?;
    Ok(())
}

#[cfg(not(unix))]
pub(super) async fn sync_parent_directory(_path: &Path) -> Result<(), AppError> {
    Ok(())
}
