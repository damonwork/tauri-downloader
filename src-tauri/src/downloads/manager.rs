use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    fs,
    io::AsyncWriteExt,
    sync::{watch, Mutex, Notify, RwLock},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    engine::{DownloadEngine, EngineError, EngineInput, EngineProgress, ResolvedProxy},
    error::AppError,
    model::{
        AppSettings, AppSnapshot, CreateDownloadInput, DownloadAction, DownloadCategory,
        DownloadItem, DownloadProgressEvent, DownloadSource, DownloadState, ProxyHealth,
        ProxyProfile, ProxySelection, ResumeSupport, RevisionEvent, SegmentState, SourceValidator,
        TransferPhase, TransferProgress, TransferSize, TransferTelemetry,
    },
};

const STORE_FILE: &str = "state.json";
const MAX_FILE_NAME_UTF16_UNITS: usize = 200;
const MAX_THREADS_PER_DOWNLOAD: u8 = 32;
const MAX_CONCURRENT_DOWNLOADS: u8 = 12;
const MAX_SPEED_LIMIT_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const PROGRESS_PERSIST_INTERVAL: Duration = Duration::from_secs(3);

struct TaskControl {
    generation: String,
    cancellation: CancellationToken,
    join: JoinHandle<()>,
}

enum JobFailure {
    Engine(EngineError),
    Persistence(AppError),
}

pub struct DownloadManager {
    app: AppHandle,
    state: RwLock<AppSnapshot>,
    tasks: Mutex<HashMap<String, TaskControl>>,
    starting: Mutex<HashSet<String>>,
    stopping: Mutex<HashSet<String>>,
    operation_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    persistence: Mutex<()>,
    progress_persisted_at: Mutex<HashMap<String, Instant>>,
    scheduler: Notify,
    store_path: PathBuf,
}

impl DownloadManager {
    pub async fn load(app: AppHandle) -> Result<Arc<Self>, AppError> {
        let app_dir = app
            .path()
            .app_data_dir()
            .map_err(|_| AppError::AppDirectory)?;
        fs::create_dir_all(&app_dir).await?;
        let store_path = app_dir.join(STORE_FILE);
        let mut snapshot = load_snapshot(&store_path).await?;

        for item in &mut snapshot.downloads {
            if matches!(&item.state, DownloadState::Downloading { .. }) {
                item.state = DownloadState::Queued;
            }
            reset_live_telemetry(item);
        }

        let manager = Arc::new(Self {
            app,
            state: RwLock::new(snapshot),
            tasks: Mutex::new(HashMap::new()),
            starting: Mutex::new(HashSet::new()),
            stopping: Mutex::new(HashSet::new()),
            operation_locks: Mutex::new(HashMap::new()),
            persistence: Mutex::new(()),
            progress_persisted_at: Mutex::new(HashMap::new()),
            scheduler: Notify::new(),
            store_path,
        });
        manager.restore_output_reservations().await;
        Ok(manager)
    }

    pub fn start_scheduler(self: Arc<Self>) {
        tauri::async_runtime::spawn(async move {
            self.scheduler.notify_one();
            loop {
                self.scheduler.notified().await;
                loop {
                    match self.next_job().await {
                        Ok(Some((item, destination_dir, proxy))) => {
                            self.spawn_job(item, destination_dir, proxy).await;
                        }
                        Ok(None) => break,
                        Err(error) => {
                            if !self.fail_first_queued(error.to_string()).await {
                                break;
                            }
                        }
                    }
                }
            }
        });
    }

    pub async fn snapshot(&self) -> AppSnapshot {
        self.state.read().await.clone()
    }

    pub async fn add(&self, mut input: CreateDownloadInput) -> Result<DownloadItem, AppError> {
        validate_source(&input.source)?;
        let (proxy, settings) = {
            let state = self.state.read().await;
            (
                resolve_proxy(&state, &input.source.proxy)?,
                state.settings.clone(),
            )
        };
        if !input.file_name_customized && needs_remote_file_name(&input.file_name) {
            if let Some(detected) = DownloadEngine::detect_file_name(&input.source, &proxy).await {
                if let Some(file_name) = sanitize_detected_file_name(&detected) {
                    input.file_name = file_name;
                }
            }
        }
        if !input.category_customized {
            input.category = category_for_file(&input.file_name);
        }
        if !input.destination_customized {
            input.destination = destination_for_category(&settings, &input.category);
        }
        validate_create_input(&input)?;
        let now = Utc::now();
        let item = DownloadItem {
            id: Uuid::new_v4().to_string(),
            file_name: input.file_name,
            category: input.category,
            state: if input.start_immediately {
                DownloadState::Queued
            } else {
                DownloadState::Paused
            },
            source: input.source,
            destination: input.destination,
            transfer: TransferProgress {
                downloaded_bytes: 0,
                size: TransferSize::Unknown,
                validator: SourceValidator::None,
                resume: ResumeSupport::Unknown,
            },
            telemetry: TransferTelemetry::default(),
            threads: input.threads,
            speed_limit_bytes: input.speed_limit_bytes,
            created_at: now,
            updated_at: now,
        };
        self.reserve_output(&item).await?;
        {
            let mut state = self.state.write().await;
            state.downloads.insert(0, item.clone());
            state.revision += 1;
        }
        if let Err(error) = self.commit().await {
            let mut state = self.state.write().await;
            state.downloads.retain(|existing| existing.id != item.id);
            state.revision += 1;
            drop(state);
            let _ = self.release_output(&item).await;
            return Err(error);
        }
        self.scheduler.notify_one();
        Ok(item)
    }

    pub async fn control(&self, id: &str, action: DownloadAction) -> Result<(), AppError> {
        let operation = self.operation_lock(id).await;
        let _guard = operation.lock().await;
        match action {
            DownloadAction::Pause => self.pause(id).await,
            DownloadAction::Resume | DownloadAction::Retry => self.queue(id).await,
            DownloadAction::Restart => self.restart(id).await,
            DownloadAction::Remove => self.remove(id).await,
        }
    }

    pub async fn replace_source(&self, id: &str, source: DownloadSource) -> Result<(), AppError> {
        let operation = self.operation_lock(id).await;
        let _guard = operation.lock().await;
        validate_source(&source)?;
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
                "La descarga ya se completó; crea una nueva descarga para cambiar el origen"
                    .to_owned(),
            ));
        }
        self.reconcile_partial_progress(id).await?;
        {
            let mut state = self.state.write().await;
            let item = find_download_mut(&mut state, id)?;
            item.source = source;
            item.transfer.resume = ResumeSupport::Unknown;
            item.telemetry = TransferTelemetry::default();
            item.state = DownloadState::Queued;
            item.updated_at = Utc::now();
            state.revision += 1;
        }
        self.commit().await?;
        self.scheduler.notify_one();
        Ok(())
    }

    pub async fn update_settings(&self, settings: AppSettings) -> Result<(), AppError> {
        validate_settings(&settings)?;
        let previous = {
            let mut state = self.state.write().await;
            let previous = std::mem::replace(&mut state.settings, settings);
            state.revision += 1;
            previous
        };
        if let Err(error) = self.commit().await {
            let mut state = self.state.write().await;
            state.settings = previous;
            state.revision += 1;
            return Err(error);
        }
        self.scheduler.notify_one();
        Ok(())
    }

    pub async fn save_proxy(&self, mut proxy: ProxyProfile) -> Result<(), AppError> {
        validate_proxy(&proxy)?;
        proxy.health = ProxyHealth::Untested;
        let previous = {
            let mut state = self.state.write().await;
            if !proxy.enabled
                && state.downloads.iter().any(|item| {
                    !matches!(&item.state, DownloadState::Completed { .. })
                        && matches!(&item.source.proxy, ProxySelection::Profile { profile_id } if profile_id == &proxy.id)
                })
            {
                return Err(AppError::Validation(
                    "El proxy está asignado a una descarga pendiente y no puede desactivarse"
                        .to_owned(),
                ));
            }
            let previous = state.proxies.clone();
            if let Some(existing) = state.proxies.iter_mut().find(|item| item.id == proxy.id) {
                *existing = proxy;
            } else {
                state.proxies.push(proxy);
            }
            state.revision += 1;
            previous
        };
        if let Err(error) = self.commit().await {
            let mut state = self.state.write().await;
            state.proxies = previous;
            state.revision += 1;
            return Err(error);
        }
        Ok(())
    }

    pub async fn remove_proxy(&self, id: &str) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        if state.downloads.iter().any(|item| {
            matches!(&item.source.proxy, ProxySelection::Profile { profile_id } if profile_id == id)
        }) {
            return Err(AppError::Validation(
                "El proxy está asignado a una descarga y no puede eliminarse".to_owned(),
            ));
        }
        let previous = state.proxies.clone();
        let previous_len = previous.len();
        state.proxies.retain(|proxy| proxy.id != id);
        if state.proxies.len() == previous_len {
            return Err(AppError::ProxyNotFound);
        }
        state.revision += 1;
        drop(state);
        if let Err(error) = self.commit().await {
            let mut state = self.state.write().await;
            state.proxies = previous;
            state.revision += 1;
            return Err(error);
        }
        Ok(())
    }

    pub async fn check_proxy(&self, id: &str) -> Result<(), AppError> {
        let proxy_url = {
            let mut state = self.state.write().await;
            let proxy = state
                .proxies
                .iter_mut()
                .find(|proxy| proxy.id == id)
                .ok_or(AppError::ProxyNotFound)?;
            if !proxy.enabled {
                proxy.health = ProxyHealth::Offline {
                    reason: "El perfil está desactivado".to_owned(),
                };
                state.revision += 1;
                drop(state);
                return self.commit().await;
            }
            proxy.health = ProxyHealth::Checking;
            let url = proxy.url.clone();
            state.revision += 1;
            url
        };
        self.commit().await?;

        let started = Instant::now();
        let health = match reqwest::Proxy::all(&proxy_url) {
            Ok(proxy) => match reqwest::Client::builder()
                .proxy(proxy)
                .timeout(Duration::from_secs(10))
                .build()
            {
                Ok(client) => match client.get("https://example.com/").send().await {
                    Ok(response) if response.status().is_success() => ProxyHealth::Online {
                        latency_ms: started.elapsed().as_millis() as u64,
                    },
                    Ok(response) => ProxyHealth::Offline {
                        reason: format!("El proxy respondió HTTP {}", response.status().as_u16()),
                    },
                    Err(_) => ProxyHealth::Offline {
                        reason: "No se pudo establecer conexión".to_owned(),
                    },
                },
                _ => ProxyHealth::Offline {
                    reason: "No se pudo establecer conexión".to_owned(),
                },
            },
            Err(_) => ProxyHealth::Offline {
                reason: "La URL del proxy no es válida".to_owned(),
            },
        };

        {
            let mut state = self.state.write().await;
            let proxy = state
                .proxies
                .iter_mut()
                .find(|proxy| proxy.id == id)
                .ok_or(AppError::ProxyNotFound)?;
            proxy.health = health;
            state.revision += 1;
        }
        self.commit().await
    }

    async fn pause(&self, id: &str) -> Result<(), AppError> {
        self.set_paused(id).await?;
        self.stop_task(id).await;
        self.reconcile_partial_progress(id).await?;
        self.commit().await?;
        self.scheduler.notify_one();
        Ok(())
    }

    async fn set_paused(&self, id: &str) -> Result<(), AppError> {
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

    async fn reconcile_partial_progress(&self, id: &str) -> Result<(), AppError> {
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

    async fn queue(&self, id: &str) -> Result<(), AppError> {
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

    async fn restart(&self, id: &str) -> Result<(), AppError> {
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

    async fn remove(&self, id: &str) -> Result<(), AppError> {
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

    async fn next_job(&self) -> Result<Option<(DownloadItem, PathBuf, ResolvedProxy)>, AppError> {
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

    async fn fail_first_queued(&self, message: String) -> bool {
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

    async fn spawn_job(
        self: &Arc<Self>,
        item: DownloadItem,
        destination_dir: PathBuf,
        proxy: ResolvedProxy,
    ) {
        let id = item.id.clone();
        let cancellation = CancellationToken::new();
        let gate = Arc::new(Notify::new());
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

    async fn update_progress(&self, id: &str, progress: EngineProgress) -> Result<(), AppError> {
        let (event, identity_changed) = {
            let mut state = self.state.write().await;
            let Ok(item) = find_download_mut(&mut state, id) else {
                return Ok(());
            };
            if !matches!(&item.state, DownloadState::Downloading { .. }) {
                return Ok(());
            }
            let identity_changed = (matches!(&item.transfer.validator, SourceValidator::None)
                && !matches!(&progress.validator, SourceValidator::None))
                || (matches!(&item.transfer.size, TransferSize::Unknown)
                    && matches!(&progress.size, TransferSize::Known { .. }));
            item.transfer.downloaded_bytes = progress.downloaded_bytes;
            item.transfer.size = progress.size;
            item.transfer.validator = progress.validator;
            item.transfer.resume = progress.resume;
            item.telemetry = progress.telemetry;
            item.state = DownloadState::Downloading {
                speed_bytes: progress.speed_bytes,
            };
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

    async fn operation_lock(&self, id: &str) -> Arc<Mutex<()>> {
        let mut operations = self.operation_locks.lock().await;
        Arc::clone(
            operations
                .entry(id.to_owned())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    async fn stop_task(&self, id: &str) {
        let control = {
            let mut tasks = self.tasks.lock().await;
            let mut stopping = self.stopping.lock().await;
            let mut starting = self.starting.lock().await;
            if !tasks.contains_key(id) {
                starting.remove(id);
                return;
            }
            starting.remove(id);
            stopping.insert(id.to_owned());
            tasks.remove(id)
        };
        if let Some(control) = control {
            control.cancellation.cancel();
            let _ = control.join.await;
            self.stopping.lock().await.remove(id);
        }
    }

    fn destination_dir(&self, item: &DownloadItem) -> Result<PathBuf, AppError> {
        let configured = Path::new(&item.destination);
        if configured
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(AppError::Validation(
                "El directorio de destino no es seguro".to_owned(),
            ));
        }
        if configured.is_absolute() {
            return Ok(configured.to_path_buf());
        }
        if configured.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err(AppError::Validation(
                "El directorio de destino no es seguro".to_owned(),
            ));
        }
        let downloads = self
            .app
            .path()
            .download_dir()
            .map_err(|_| AppError::AppDirectory)?;
        Ok(downloads.join(configured))
    }

    async fn reserve_output(&self, item: &DownloadItem) -> Result<(), AppError> {
        let directory = self.destination_dir(item)?;
        fs::create_dir_all(&directory).await?;
        if !matches!(&item.state, DownloadState::Completed { .. })
            && fs::try_exists(directory.join(&item.file_name)).await?
        {
            return Err(AppError::Validation(
                "El archivo de destino ya existe".to_owned(),
            ));
        }
        let reservation = reservation_path(&directory, &item.file_name);
        match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&reservation)
            .await
        {
            Ok(mut file) => {
                let result = async {
                    set_private_permissions(&reservation).await?;
                    file.write_all(item.id.as_bytes()).await?;
                    file.flush().await?;
                    file.sync_all().await?;
                    Ok(())
                }
                .await;
                if result.is_err() {
                    drop(file);
                    let _ = remove_if_exists(&reservation).await;
                }
                result
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let owner = fs::read_to_string(&reservation).await.unwrap_or_default();
                if owner == item.id {
                    Ok(())
                } else {
                    Err(AppError::Validation(
                        "Otro elemento ya reservó el mismo archivo de destino".to_owned(),
                    ))
                }
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn release_output(&self, item: &DownloadItem) -> Result<(), AppError> {
        let directory = self.destination_dir(item)?;
        let reservation = reservation_path(&directory, &item.file_name);
        match fs::read_to_string(&reservation).await {
            Ok(owner) if owner == item.id => remove_if_exists(&reservation).await,
            Ok(_) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    async fn restore_output_reservations(&self) {
        let downloads = self.state.read().await.downloads.clone();
        let mut changed = false;
        for item in downloads {
            if let Err(error) = self.reserve_output(&item).await {
                let mut state = self.state.write().await;
                if let Ok(current) = find_download_mut(&mut state, &item.id) {
                    current.state = DownloadState::Failed {
                        message: error.to_string(),
                        recoverable: false,
                    };
                    current.updated_at = Utc::now();
                    state.revision += 1;
                    changed = true;
                }
            }
        }
        if changed {
            let _ = self.persist().await;
        }
    }

    async fn commit(&self) -> Result<(), AppError> {
        self.persist().await?;
        let revision = self.state.read().await.revision;
        self.emit_revision(revision);
        Ok(())
    }

    async fn persist(&self) -> Result<(), AppError> {
        let _guard = self.persistence.lock().await;
        let snapshot = self.state.read().await.clone();
        let bytes = serde_json::to_vec_pretty(&snapshot)?;
        persist_snapshot(&self.store_path, &bytes, snapshot.revision).await
    }

    fn emit_revision(&self, revision: u64) {
        let _ = self
            .app
            .emit("downloads://changed", RevisionEvent { revision });
    }

    fn emit_progress(&self, event: DownloadProgressEvent) {
        let _ = self.app.emit("downloads://progress", event);
    }

    async fn should_persist_progress(&self, id: &str, force: bool) -> bool {
        let now = Instant::now();
        let mut persisted = self.progress_persisted_at.lock().await;
        let due = force
            || persisted
                .get(id)
                .is_none_or(|last| now.duration_since(*last) >= PROGRESS_PERSIST_INTERVAL);
        if due {
            persisted.insert(id.to_owned(), now);
        }
        due
    }
}

async fn persist_snapshot(path: &Path, bytes: &[u8], revision: u64) -> Result<(), AppError> {
    let temporary = path.with_extension(format!("json.{revision}.tmp"));
    let result = async {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .await?;
        file.write_all(bytes).await?;
        file.flush().await?;
        set_private_permissions(&temporary).await?;
        file.sync_all().await?;
        drop(file);

        if let Err(error) = fs::rename(&temporary, path).await {
            if !fs::try_exists(path).await? {
                return Err(error.into());
            }
            let backup = path.with_extension("json.bak");
            remove_if_exists(&backup).await?;
            fs::rename(path, &backup).await?;
            if let Err(replace_error) = fs::rename(&temporary, path).await {
                let _ = fs::rename(&backup, path).await;
                return Err(replace_error.into());
            }
            let _ = remove_if_exists(&backup).await;
        }
        sync_parent_directory(path).await
    }
    .await;

    if result.is_err() {
        let _ = remove_if_exists(&temporary).await;
    }
    result
}

async fn load_snapshot(path: &Path) -> Result<AppSnapshot, AppError> {
    match fs::read(path).await {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(snapshot) => Ok(snapshot),
            Err(primary_error) => {
                let backup = path.with_extension("json.bak");
                match fs::read(backup).await {
                    Ok(backup_bytes) => Ok(serde_json::from_slice(&backup_bytes)?),
                    Err(_) => Err(primary_error.into()),
                }
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let backup = path.with_extension("json.bak");
            match fs::read(backup).await {
                Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
                Err(backup_error) if backup_error.kind() == std::io::ErrorKind::NotFound => {
                    Ok(AppSnapshot::default())
                }
                Err(backup_error) => Err(backup_error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

fn find_download<'a>(state: &'a AppSnapshot, id: &str) -> Result<&'a DownloadItem, AppError> {
    state
        .downloads
        .iter()
        .find(|item| item.id == id)
        .ok_or(AppError::DownloadNotFound)
}

fn find_download_mut<'a>(
    state: &'a mut AppSnapshot,
    id: &str,
) -> Result<&'a mut DownloadItem, AppError> {
    state
        .downloads
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or(AppError::DownloadNotFound)
}

fn resolve_proxy(
    state: &AppSnapshot,
    selection: &ProxySelection,
) -> Result<ResolvedProxy, AppError> {
    match selection {
        ProxySelection::Direct => Ok(ResolvedProxy::Direct),
        ProxySelection::Profile { profile_id } => {
            let profile = state
                .proxies
                .iter()
                .find(|proxy| proxy.id == *profile_id)
                .ok_or(AppError::ProxyNotFound)?;
            if !profile.enabled {
                return Err(AppError::Validation(
                    "El proxy asignado está desactivado".to_owned(),
                ));
            }
            Ok(ResolvedProxy::Url(profile.url.clone()))
        }
    }
}

fn validate_create_input(input: &CreateDownloadInput) -> Result<(), AppError> {
    if input.threads == 0 || input.threads > MAX_THREADS_PER_DOWNLOAD {
        return Err(AppError::Validation(
            "Los hilos deben estar entre 1 y 32".to_owned(),
        ));
    }
    if input.speed_limit_bytes > MAX_SPEED_LIMIT_BYTES {
        return Err(AppError::Validation(
            "El límite de velocidad supera el máximo permitido".to_owned(),
        ));
    }
    if input.file_name.encode_utf16().count() > MAX_FILE_NAME_UTF16_UNITS {
        return Err(AppError::Validation(format!(
            "El nombre del archivo no puede superar {MAX_FILE_NAME_UTF16_UNITS} caracteres"
        )));
    }
    if input.file_name.trim().is_empty()
        || input
            .file_name
            .chars()
            .any(|character| character.is_control() || "<>:\"/\\|?*".contains(character))
        || input.file_name.contains("..")
        || input.file_name.ends_with(['.', ' '])
        || is_windows_reserved_name(&input.file_name)
    {
        return Err(AppError::Validation(
            "El nombre del archivo no es válido".to_owned(),
        ));
    }
    if input.destination.trim().is_empty() {
        return Err(AppError::Validation(
            "El directorio de destino es obligatorio".to_owned(),
        ));
    }
    Ok(())
}

fn is_windows_reserved_name(file_name: &str) -> bool {
    let stem = file_name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .and_then(|value| value.parse::<u8>().ok())
            .is_some_and(|value| (1..=9).contains(&value))
}

fn sanitize_detected_file_name(value: &str) -> Option<String> {
    let base_name = value.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut sanitized = String::new();
    let mut previous_whitespace = false;
    for character in base_name.chars() {
        if character.is_control() || "<>:\"/\\|?*".contains(character) {
            sanitized.push('_');
            previous_whitespace = false;
        } else if character.is_whitespace() {
            if !previous_whitespace {
                sanitized.push(' ');
            }
            previous_whitespace = true;
        } else {
            sanitized.push(character);
            previous_whitespace = false;
        }
    }
    let mut sanitized = sanitized
        .trim()
        .trim_end_matches(['.', ' '])
        .replace("..", "._");
    if sanitized.is_empty() {
        return None;
    }
    if is_windows_reserved_name(&sanitized) {
        sanitized.insert(0, '_');
    }
    Some(truncate_file_name(&sanitized, MAX_FILE_NAME_UTF16_UNITS))
}

fn needs_remote_file_name(file_name: &str) -> bool {
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    extension.is_empty()
        || matches!(
            extension.as_str(),
            "php" | "asp" | "aspx" | "cgi" | "htm" | "html"
        )
        || matches!(stem.as_str(), "download" | "file" | "get" | "index")
}

fn truncate_file_name(value: &str, max_units: usize) -> String {
    if value.encode_utf16().count() <= max_units {
        return value.to_owned();
    }
    let extension = value
        .rfind('.')
        .filter(|index| *index > 0)
        .map(|index| &value[index..])
        .filter(|extension| extension.encode_utf16().count() <= 20)
        .unwrap_or("");
    let stem = &value[..value.len() - extension.len()];
    let budget = max_units.saturating_sub(extension.encode_utf16().count());
    let mut output = String::new();
    let mut units = 0;
    for character in stem.chars() {
        let next = character.len_utf16();
        if units + next > budget {
            break;
        }
        output.push(character);
        units += next;
    }
    format!("{}{extension}", output.trim_end_matches(['.', ' ']))
}

fn category_for_file(file_name: &str) -> DownloadCategory {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "mp4" | "mkv" | "mov" | "webm" | "avi" => DownloadCategory::Video,
        "zip" | "rar" | "7z" | "tar" | "gz" => DownloadCategory::Archive,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "txt" => DownloadCategory::Document,
        "mp3" | "wav" | "flac" | "m4a" | "ogg" => DownloadCategory::Audio,
        _ => DownloadCategory::Other,
    }
}

fn destination_for_category(settings: &AppSettings, category: &DownloadCategory) -> String {
    if !settings.organize_by_category {
        return settings.download_directory.clone();
    }
    let category_directory = match category {
        DownloadCategory::Video => &settings.category_directories.video,
        DownloadCategory::Archive => &settings.category_directories.archive,
        DownloadCategory::Document => &settings.category_directories.document,
        DownloadCategory::Audio => &settings.category_directories.audio,
        DownloadCategory::Other => &settings.category_directories.other,
    };
    Path::new(&settings.download_directory)
        .join(category_directory)
        .to_string_lossy()
        .into_owned()
}

fn reservation_path(directory: &Path, file_name: &str) -> PathBuf {
    directory.join(format!(".{file_name}.fluxor.lock"))
}

fn validate_source(source: &DownloadSource) -> Result<(), AppError> {
    let url = reqwest::Url::parse(&source.url)
        .map_err(|_| AppError::Validation("El enlace no es válido".to_owned()))?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(AppError::Validation(
            "Solo se permiten enlaces HTTP o HTTPS".to_owned(),
        ));
    }
    if source.headers.iter().any(|header| {
        header.name.trim().is_empty()
            || header.name.contains(['\r', '\n'])
            || header.value.contains(['\r', '\n'])
    }) {
        return Err(AppError::Validation(
            "Uno de los headers no es válido".to_owned(),
        ));
    }
    if let Some(header) = source
        .headers
        .iter()
        .find(|header| is_engine_controlled_header(&header.name))
    {
        return Err(AppError::Validation(format!(
            "El header {} está controlado por Fluxor",
            header.name
        )));
    }
    if source.cookies.iter().any(|cookie| {
        cookie.name.trim().is_empty()
            || cookie.name.contains([';', '\r', '\n'])
            || cookie.value.contains(['\r', '\n'])
    }) {
        return Err(AppError::Validation(
            "Una de las cookies no es válida".to_owned(),
        ));
    }
    Ok(())
}

fn is_engine_controlled_header(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "accept-encoding"
            | "connection"
            | "content-length"
            | "content-range"
            | "cookie"
            | "host"
            | "if-range"
            | "range"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn validate_settings(settings: &AppSettings) -> Result<(), AppError> {
    if settings.max_concurrent == 0 || settings.max_concurrent > MAX_CONCURRENT_DOWNLOADS {
        return Err(AppError::Validation(
            "Las descargas simultáneas deben estar entre 1 y 12".to_owned(),
        ));
    }
    if settings.default_threads == 0 || settings.default_threads > MAX_THREADS_PER_DOWNLOAD {
        return Err(AppError::Validation(
            "Los hilos predeterminados deben estar entre 1 y 32".to_owned(),
        ));
    }
    if settings.default_speed_limit_bytes > MAX_SPEED_LIMIT_BYTES {
        return Err(AppError::Validation(
            "El límite de velocidad predeterminado no es válido".to_owned(),
        ));
    }
    if !is_safe_configured_directory(&settings.download_directory) {
        return Err(AppError::Validation(
            "El directorio principal de descarga no es válido".to_owned(),
        ));
    }
    if settings.organize_by_category
        && [
            &settings.category_directories.video,
            &settings.category_directories.archive,
            &settings.category_directories.document,
            &settings.category_directories.audio,
            &settings.category_directories.other,
        ]
        .into_iter()
        .any(|directory| !is_safe_relative_subdirectory(directory))
    {
        return Err(AppError::Validation(
            "Una de las carpetas de categoría no es válida".to_owned(),
        ));
    }
    Ok(())
}

fn is_safe_configured_directory(value: &str) -> bool {
    let path = Path::new(value.trim());
    !value.trim().is_empty()
        && !path.components().any(|component| {
            if path.is_absolute() {
                matches!(component, Component::ParentDir)
            } else {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            }
        })
}

fn is_safe_relative_subdirectory(value: &str) -> bool {
    let path = Path::new(value.trim());
    !value.trim().is_empty()
        && !path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

fn validate_proxy(proxy: &ProxyProfile) -> Result<(), AppError> {
    if proxy.id.trim().is_empty() || proxy.name.trim().is_empty() {
        return Err(AppError::Validation(
            "El proxy necesita nombre e identificador".to_owned(),
        ));
    }
    reqwest::Proxy::all(&proxy.url)
        .map_err(|_| AppError::Validation("La URL del proxy no es válida".to_owned()))?;
    Ok(())
}

async fn remove_partial_files(directory: &Path, file_name: &str) -> Result<(), AppError> {
    let partial = directory.join(format!(".{file_name}.fluxor.part"));
    remove_if_exists(&partial).await?;
    for index in 0..usize::from(MAX_THREADS_PER_DOWNLOAD) {
        let segment = directory.join(format!(".{file_name}.fluxor.part.{index}"));
        remove_if_exists(&segment).await?;
    }
    Ok(())
}

async fn remove_if_exists(path: &Path) -> Result<(), AppError> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

async fn stored_file_len(path: &Path) -> Result<u64, AppError> {
    match fs::metadata(path).await {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

fn reset_live_telemetry(item: &mut DownloadItem) {
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

fn pause_telemetry(item: &mut DownloadItem) {
    item.telemetry.phase = TransferPhase::Idle;
    for segment in &mut item.telemetry.segments {
        segment.speed_bytes = 0;
        if !matches!(segment.state, SegmentState::Completed) {
            segment.state = SegmentState::Paused;
        }
    }
}

#[cfg(unix)]
async fn set_private_permissions(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
    Ok(())
}

#[cfg(not(unix))]
async fn set_private_permissions(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

#[cfg(unix)]
async fn sync_parent_directory(path: &Path) -> Result<(), AppError> {
    let parent = path.parent().ok_or(AppError::AppDirectory)?;
    let directory = fs::File::open(parent).await?;
    directory.sync_all().await?;
    Ok(())
}

#[cfg(not(unix))]
async fn sync_parent_directory(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        destination_for_category, is_engine_controlled_header, is_safe_configured_directory,
        is_safe_relative_subdirectory, is_windows_reserved_name, needs_remote_file_name,
        persist_snapshot, sanitize_detected_file_name, MAX_FILE_NAME_UTF16_UNITS,
    };
    use crate::downloads::model::{AppSettings, DownloadCategory};

    #[test]
    fn rejects_windows_reserved_file_names_on_every_platform() {
        assert!(is_windows_reserved_name("CON.txt"));
        assert!(is_windows_reserved_name("lpt9.log"));
        assert!(!is_windows_reserved_name("console.txt"));
    }

    #[test]
    fn configured_directories_reject_parent_traversal() {
        assert!(is_safe_configured_directory("Fluxor/Videos"));
        assert!(!is_safe_configured_directory("../outside"));
        assert!(!is_safe_relative_subdirectory("../Videos"));
        assert!(is_safe_relative_subdirectory("Media/Videos"));
    }

    #[test]
    fn detected_file_names_are_sanitized_and_bounded() {
        let detected = format!("../{}:video?.mp4", "a".repeat(250));
        let file_name = sanitize_detected_file_name(&detected).unwrap();

        assert!(file_name.encode_utf16().count() <= MAX_FILE_NAME_UTF16_UNITS);
        assert!(file_name.ends_with(".mp4"));
        assert!(!file_name.contains(['/', '\\', ':', '?']));
    }

    #[test]
    fn automatic_destination_tracks_detected_category() {
        let settings = AppSettings::default();

        assert_eq!(
            destination_for_category(&settings, &DownloadCategory::Video),
            std::path::Path::new("Fluxor")
                .join("Videos")
                .to_string_lossy()
        );
    }

    #[test]
    fn remote_metadata_is_only_needed_for_generic_names() {
        assert!(needs_remote_file_name("download"));
        assert!(needs_remote_file_name("download.php"));
        assert!(!needs_remote_file_name("video.mp4"));
    }

    #[test]
    fn transport_headers_are_engine_controlled() {
        assert!(is_engine_controlled_header("Accept-Encoding"));
        assert!(is_engine_controlled_header("Cookie"));
        assert!(!is_engine_controlled_header("User-Agent"));
        assert!(!is_engine_controlled_header("Referer"));
    }

    #[tokio::test]
    async fn persisted_snapshot_replaces_existing_file() {
        let directory = std::env::temp_dir().join(format!("fluxor-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&directory).await.unwrap();
        let path = directory.join("state.json");

        persist_snapshot(&path, b"first", 1).await.unwrap();
        persist_snapshot(&path, b"second", 2).await.unwrap();

        assert_eq!(tokio::fs::read(&path).await.unwrap(), b"second");
        tokio::fs::remove_dir_all(directory).await.unwrap();
    }
}
