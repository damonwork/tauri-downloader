use chrono::Utc;

use super::files::{pause_telemetry, remove_partial_files, reset_live_telemetry, stored_file_len};
use super::validation::{find_download, find_download_mut};
use super::{
    AppError, DownloadManager, DownloadState, ResumeSupport, SegmentState, SourceValidator,
    TransferPhase, TransferProgress, TransferSize, TransferTelemetry,
};

impl DownloadManager {
    pub(super) async fn pause(&self, id: &str) -> Result<(), AppError> {
        self.set_paused(id).await?;
        self.stop_task(id).await;
        self.reconcile_partial_progress(id).await?;
        self.commit().await?;
        self.scheduler.notify_one();
        Ok(())
    }

    pub(super) async fn set_paused(&self, id: &str) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        let item = find_download_mut(&mut state, id)?;
        if !matches!(&item.state, DownloadState::Completed { .. }) {
            item.state = DownloadState::Paused;
            pause_telemetry(item);
            item.updated_at = Utc::now();
            state.revision += 1;
        }
        Ok(())
    }

    pub(super) async fn reconcile_partial_progress(&self, id: &str) -> Result<(), AppError> {
        let (directory, file_name, threads, recorded) = {
            let state = self.state.read().await;
            let item = find_download(&state, id)?;
            if matches!(&item.state, DownloadState::Completed { .. }) {
                return Ok(());
            }
            (
                self.destination_dir(item)?,
                item.file_name.clone(),
                item.threads,
                item.transfer.downloaded_bytes,
            )
        };
        let single = stored_file_len(&directory.join(format!(".{file_name}.fluxor.part"))).await?;
        let mut segmented = 0_u64;
        let mut segment_lengths = Vec::with_capacity(usize::from(threads));
        for index in 0..usize::from(threads) {
            let length =
                stored_file_len(&directory.join(format!(".{file_name}.fluxor.part.{index}")))
                    .await?;
            segmented = segmented.saturating_add(length);
            segment_lengths.push(length);
        }
        let downloaded = single.max(segmented);
        if downloaded != recorded || single > 0 || segmented > 0 {
            let mut state = self.state.write().await;
            let item = find_download_mut(&mut state, id)?;
            if matches!(&item.state, DownloadState::Completed { .. }) {
                return Ok(());
            }
            let failed = matches!(&item.state, DownloadState::Failed { .. });
            item.transfer.downloaded_bytes = downloaded;
            item.telemetry.phase = TransferPhase::Idle;
            for segment in &mut item.telemetry.segments {
                let stored = if single > 0 {
                    single
                } else {
                    segment_lengths
                        .get(usize::from(segment.index))
                        .copied()
                        .unwrap_or(segment.downloaded_bytes)
                };
                let expected = segment
                    .end_byte
                    .map(|end| end.saturating_sub(segment.start_byte) + 1);
                segment.downloaded_bytes = expected.map_or(stored, |total| stored.min(total));
                segment.speed_bytes = 0;
                segment.state = if expected == Some(segment.downloaded_bytes) {
                    SegmentState::Completed
                } else if failed && matches!(segment.state, SegmentState::Failed) {
                    SegmentState::Failed
                } else if failed {
                    SegmentState::Stopped
                } else {
                    SegmentState::Paused
                };
            }
            item.updated_at = Utc::now();
            state.revision += 1;
        }
        Ok(())
    }

    pub(super) async fn queue(&self, id: &str) -> Result<(), AppError> {
        {
            let mut state = self.state.write().await;
            let item = find_download_mut(&mut state, id)?;
            if matches!(
                &item.state,
                DownloadState::Completed { .. }
                    | DownloadState::Downloading { .. }
                    | DownloadState::Queued
            ) {
                return Ok(());
            }
            item.state = DownloadState::Queued;
            reset_live_telemetry(item);
            item.updated_at = Utc::now();
            state.revision += 1;
        }
        self.commit().await?;
        self.scheduler.notify_one();
        Ok(())
    }

    pub(super) async fn restart(&self, id: &str) -> Result<(), AppError> {
        self.set_paused(id).await?;
        self.stop_task(id).await;
        let completed = {
            let state = self.state.read().await;
            matches!(
                &find_download(&state, id)?.state,
                DownloadState::Completed { .. }
            )
        };
        if completed {
            return Err(AppError::Validation(
                "La descarga terminó mientras se finalizaba el archivo".to_owned(),
            ));
        }
        let (directory, file_name) = {
            let state = self.state.read().await;
            let item = find_download(&state, id)?;
            (self.destination_dir(item)?, item.file_name.clone())
        };
        remove_partial_files(&directory, &file_name).await?;
        {
            let mut state = self.state.write().await;
            let item = find_download_mut(&mut state, id)?;
            item.transfer = TransferProgress {
                downloaded_bytes: 0,
                size: TransferSize::Unknown,
                validator: SourceValidator::None,
                resume: ResumeSupport::Unknown,
            };
            item.telemetry = TransferTelemetry::default();
            item.state = DownloadState::Queued;
            item.updated_at = Utc::now();
            state.revision += 1;
        }
        self.commit().await?;
        self.scheduler.notify_one();
        Ok(())
    }

    pub(super) async fn remove(&self, id: &str) -> Result<(), AppError> {
        self.set_paused(id).await?;
        self.stop_task(id).await;
        let removed = {
            let mut state = self.state.write().await;
            let position = state
                .downloads
                .iter()
                .position(|item| item.id == id)
                .ok_or(AppError::DownloadNotFound)?;
            let item = state.downloads.remove(position);
            state.revision += 1;
            item
        };
        let directory = self.destination_dir(&removed)?;
        if let Err(error) = self.commit().await {
            let mut state = self.state.write().await;
            state.downloads.push(removed);
            state.revision += 1;
            return Err(error);
        }
        remove_partial_files(&directory, &removed.file_name).await?;
        self.release_output(&removed).await?;
        self.scheduler.notify_one();
        Ok(())
    }
}
