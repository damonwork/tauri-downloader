use chrono::Utc;

use super::validation::find_download_mut;
use super::{DownloadManager, DownloadProgressEvent, DownloadState, SourceValidator, TransferSize};
use crate::downloads::engine::EngineProgress;

impl DownloadManager {
    pub(super) async fn update_progress(
        &self,
        id: &str,
        progress: EngineProgress,
    ) -> Result<(), super::AppError> {
        let (event, identity_changed) = {
            let mut state = self.state.write().await;
            let Ok(item) = find_download_mut(&mut state, id) else {
                return Ok(());
            };
            let identity_changed = (matches!(&item.transfer.validator, SourceValidator::None)
                && !matches!(&progress.validator, SourceValidator::None))
                || (matches!(&item.transfer.size, TransferSize::Unknown)
                    && matches!(&progress.size, TransferSize::Known { .. }));
            let downloading = matches!(&item.state, DownloadState::Downloading { .. });
            if !downloading && !identity_changed {
                return Ok(());
            }
            if downloading {
                item.transfer.downloaded_bytes = progress.downloaded_bytes;
                item.transfer.size = progress.size;
                item.transfer.validator = progress.validator;
                item.transfer.resume = progress.resume;
                item.telemetry = progress.telemetry;
                item.state = DownloadState::Downloading {
                    speed_bytes: progress.speed_bytes,
                };
            } else {
                if matches!(&item.transfer.size, TransferSize::Unknown) {
                    item.transfer.size = progress.size;
                }
                if matches!(&item.transfer.validator, SourceValidator::None) {
                    item.transfer.validator = progress.validator;
                }
                item.transfer.resume = progress.resume;
            }
            item.updated_at = Utc::now();
            let event_data = (
                item.id.clone(),
                item.state.clone(),
                item.transfer.clone(),
                item.telemetry.clone(),
                item.updated_at,
            );
            state.revision += 1;
            let (download_id, item_state, transfer, telemetry, updated_at) = event_data;
            (
                DownloadProgressEvent {
                    revision: state.revision,
                    download_id,
                    state: item_state,
                    transfer,
                    telemetry,
                    updated_at,
                },
                identity_changed,
            )
        };
        self.emit_progress(event);
        if self.should_persist_progress(id, identity_changed).await {
            self.persist().await?;
        }
        Ok(())
    }
}
