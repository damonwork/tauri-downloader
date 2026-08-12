use std::{collections::HashSet, path::PathBuf, sync::Arc};

use chrono::Utc;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::validation::{find_download_mut, resolve_proxy};
use super::{
    DownloadItem, DownloadManager, DownloadProgressEvent, DownloadState, EngineError, JobFailure,
    ResolvedProxy, SegmentState, SourceValidator, TaskControl, TransferPhase, TransferSize,
};
use crate::downloads::engine::{DownloadEngine, EngineInput, EngineProgress};

impl DownloadManager {
    pub(super) async fn next_job(
        &self,
    ) -> Result<Option<(DownloadItem, PathBuf, ResolvedProxy)>, super::AppError> {
        let tasks = self.tasks.lock().await;
        let stopping = self.stopping.lock().await;
        let mut starting = self.starting.lock().await;
        let active = tasks.len() + stopping.len() + starting.len();
        let blocked = tasks
            .keys()
            .chain(stopping.iter())
            .chain(starting.iter())
            .cloned()
            .collect::<HashSet<_>>();
        let state = self.state.read().await;
        if active >= usize::from(state.settings.max_concurrent) {
            return Ok(None);
        }
        let Some(index) = state.downloads.iter().position(|item| {
            matches!(&item.state, DownloadState::Queued) && !blocked.contains(&item.id)
        }) else {
            return Ok(None);
        };

        let item = state.downloads[index].clone();
        let proxy = resolve_proxy(&state, &item.source.proxy)?;
        let destination = self.destination_dir(&item)?;
        starting.insert(item.id.clone());
        drop(state);
        drop(starting);
        drop(stopping);
        drop(tasks);
        let marked_downloading = {
            let mut state = self.state.write().await;
            if let Ok(current) = find_download_mut(&mut state, &item.id) {
                if !matches!(&current.state, DownloadState::Queued) {
                    false
                } else {
                    current.state = DownloadState::Downloading { speed_bytes: 0 };
                    current.telemetry.phase = TransferPhase::Preparing;
                    current.updated_at = Utc::now();
                    state.revision += 1;
                    true
                }
            } else {
                false
            }
        };
        if !marked_downloading {
            self.starting.lock().await.remove(&item.id);
            return Ok(None);
        }
        if let Err(error) = self.commit().await {
            let revision = {
                let mut state = self.state.write().await;
                if let Ok(current) = find_download_mut(&mut state, &item.id) {
                    current.state = DownloadState::Failed {
                        message: error.to_string(),
                        recoverable: false,
                    };
                    current.updated_at = Utc::now();
                    state.revision += 1;
                    Some(state.revision)
                } else {
                    None
                }
            };
            self.starting.lock().await.remove(&item.id);
            if let Some(revision) = revision {
                self.emit_revision(revision);
            }
            self.scheduler.notify_one();
            return Ok(None);
        }
        Ok(Some((item, destination, proxy)))
    }

    pub(super) async fn fail_first_queued(&self, message: String) -> bool {
        let revision = {
            let mut state = self.state.write().await;
            let Some(item) = state
                .downloads
                .iter_mut()
                .find(|item| matches!(&item.state, DownloadState::Queued))
            else {
                return false;
            };
            item.state = DownloadState::Failed {
                message,
                recoverable: true,
            };
            item.updated_at = Utc::now();
            state.revision += 1;
            state.revision
        };
        self.emit_revision(revision);
        let _ = self.persist().await;
        true
    }

    pub(super) async fn spawn_job(
        self: &Arc<Self>,
        item: DownloadItem,
        destination_dir: PathBuf,
        proxy: ResolvedProxy,
    ) {
        let id = item.id.clone();
        let cancellation = CancellationToken::new();
        let gate = Arc::new(tokio::sync::Notify::new());
        let generation = Uuid::new_v4().to_string();
        let manager = Arc::clone(self);
        let task_cancellation = cancellation.clone();
        let task_gate = Arc::clone(&gate);
        let task_id = id.clone();
        let task_generation = generation.clone();
        let join = tauri::async_runtime::spawn(async move {
            task_gate.notified().await;
            manager
                .run_job(
                    task_id.clone(),
                    EngineInput {
                        item,
                        destination_dir,
                        proxy,
                    },
                    task_cancellation,
                )
                .await;
            let mut tasks = manager.tasks.lock().await;
            if tasks
                .get(&task_id)
                .is_some_and(|control| control.generation == task_generation)
            {
                tasks.remove(&task_id);
            }
            drop(tasks);
            manager.scheduler.notify_one();
        });
        let mut tasks = self.tasks.lock().await;
        let stopping = self.stopping.lock().await;
        let mut starting = self.starting.lock().await;
        let state = self.state.read().await;
        let can_start = starting.contains(&id)
            && !tasks.contains_key(&id)
            && !stopping.contains(&id)
            && state.downloads.iter().any(|download| {
                download.id == id && matches!(&download.state, DownloadState::Downloading { .. })
            });
        drop(state);
        if !can_start {
            starting.remove(&id);
            cancellation.cancel();
            gate.notify_one();
            drop(starting);
            drop(stopping);
            drop(tasks);
            let _ = join.await;
            self.scheduler.notify_one();
            return;
        }
        starting.remove(&id);
        let replaced = tasks.insert(
            id,
            TaskControl {
                generation,
                cancellation,
                join,
            },
        );
        debug_assert!(replaced.is_none());
        drop(starting);
        drop(stopping);
        drop(tasks);
        gate.notify_one();
    }

    async fn run_job(&self, id: String, input: EngineInput, cancellation: CancellationToken) {
        let (sender, mut receiver) = watch::channel(None);
        let progress_guard = sender.clone();
        let transfer = DownloadEngine::run(input, cancellation, sender);
        tokio::pin!(transfer);
        let mut progress_open = true;

        let mut result: Result<_, JobFailure> = loop {
            tokio::select! {
                changed = receiver.changed(), if progress_open => {
                    if changed.is_err() {
                        progress_open = false;
                        continue;
                    }
                    let progress = receiver.borrow_and_update().clone();
                    if let Some(progress) = progress {
                        if let Err(error) = self.update_progress(&id, progress).await {
                            break Err(JobFailure::Persistence(error));
                        }
                    }
                }
                result = &mut transfer => break result.map_err(JobFailure::Engine),
            }
        };

        if receiver.has_changed().unwrap_or(false) {
            let final_progress = { receiver.borrow_and_update().clone() };
            if let Some(progress) = final_progress {
                if let Err(error) = self.update_progress(&id, progress).await {
                    result = Err(JobFailure::Persistence(error));
                }
            }
        }
        drop(progress_guard);

        let mut cancelled = false;
        let mut failed = false;
        match result {
            Ok(output) => {
                let mut state = self.state.write().await;
                if let Ok(item) = find_download_mut(&mut state, &id) {
                    item.transfer.downloaded_bytes = output.downloaded_bytes;
                    item.transfer.size = output.size;
                    item.transfer.validator = output.validator;
                    item.transfer.resume = output.resume;
                    item.telemetry = output.telemetry;
                    item.state = DownloadState::Completed {
                        completed_at: Utc::now(),
                    };
                    item.updated_at = Utc::now();
                    state.revision += 1;
                }
            }
            Err(JobFailure::Engine(EngineError::Cancelled)) => cancelled = true,
            Err(failure) => {
                failed = true;
                let (message, recoverable) = match failure {
                    JobFailure::Engine(error) => (error.to_string(), error.recoverable()),
                    JobFailure::Persistence(error) => (error.to_string(), false),
                };
                let mut state = self.state.write().await;
                if let Ok(item) = find_download_mut(&mut state, &id) {
                    if matches!(&item.state, DownloadState::Downloading { .. }) {
                        item.state = DownloadState::Failed {
                            message,
                            recoverable,
                        };
                        item.telemetry.phase = TransferPhase::Idle;
                        for segment in &mut item.telemetry.segments {
                            segment.speed_bytes = 0;
                            if matches!(
                                segment.state,
                                SegmentState::Pending
                                    | SegmentState::Connecting
                                    | SegmentState::Downloading
                            ) {
                                segment.state = SegmentState::Stopped;
                            }
                        }
                        item.updated_at = Utc::now();
                        state.revision += 1;
                    }
                }
            }
        }
        self.progress_persisted_at.lock().await.remove(&id);
        if cancelled {
            return;
        }
        if failed {
            let _ = self.reconcile_partial_progress(&id).await;
        }
        if let Err(error) = self.commit().await {
            let revision = {
                let mut state = self.state.write().await;
                if let Ok(item) = find_download_mut(&mut state, &id) {
                    item.state = DownloadState::Failed {
                        message: format!("No se pudo guardar el estado: {error}"),
                        recoverable: false,
                    };
                    item.updated_at = Utc::now();
                    state.revision += 1;
                }
                state.revision
            };
            self.emit_revision(revision);
        }
    }

    async fn update_progress(
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
