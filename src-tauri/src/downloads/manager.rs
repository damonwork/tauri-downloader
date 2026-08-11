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
    sync::{mpsc, Mutex, Notify, RwLock},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    engine::{DownloadEngine, EngineError, EngineInput, EngineProgress, ResolvedProxy},
    error::AppError,
    model::{
        AppSettings, AppSnapshot, CreateDownloadInput, DownloadAction, DownloadItem,
        DownloadSource, DownloadState, ProxyHealth, ProxyProfile, ProxySelection, RevisionEvent,
        SourceValidator, TransferProgress, TransferSize,
    },
};

const STORE_FILE: &str = "state.json";
const MAX_THREADS_PER_DOWNLOAD: u8 = 32;
const MAX_CONCURRENT_DOWNLOADS: u8 = 12;

struct TaskControl {
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
    stopping: Mutex<HashSet<String>>,
    persistence: Mutex<()>,
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
        }

        let manager = Arc::new(Self {
            app,
            state: RwLock::new(snapshot),
            tasks: Mutex::new(HashMap::new()),
            stopping: Mutex::new(HashSet::new()),
            persistence: Mutex::new(()),
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

    pub async fn add(&self, input: CreateDownloadInput) -> Result<DownloadItem, AppError> {
        validate_create_input(&input)?;
        validate_source(&input.source)?;
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
            },
            threads: input.threads,
            created_at: now,
            updated_at: now,
        };
        {
            let state = self.state.read().await;
            resolve_proxy(&state, &item.source.proxy)?;
        }
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
        match action {
            DownloadAction::Pause => self.pause(id).await,
            DownloadAction::Resume | DownloadAction::Retry => self.queue(id).await,
            DownloadAction::Restart => self.restart(id).await,
            DownloadAction::Remove => self.remove(id).await,
        }
    }

    pub async fn replace_source(&self, id: &str, source: DownloadSource) -> Result<(), AppError> {
        validate_source(&source)?;
        self.set_paused(id).await?;
        self.stop_task(id).await;
        {
            let mut state = self.state.write().await;
            let item = find_download_mut(&mut state, id)?;
            item.source = source;
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
        self.commit().await?;
        self.scheduler.notify_one();
        Ok(())
    }

    async fn set_paused(&self, id: &str) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        let item = find_download_mut(&mut state, id)?;
        if !matches!(&item.state, DownloadState::Completed { .. }) {
            item.state = DownloadState::Paused;
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
            };
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
        let active = self.tasks.lock().await.len() + self.stopping.lock().await.len();
        let state = self.state.read().await;
        if active >= usize::from(state.settings.max_concurrent) {
            return Ok(None);
        }
        let Some(index) = state
            .downloads
            .iter()
            .position(|item| matches!(&item.state, DownloadState::Queued))
        else {
            return Ok(None);
        };

        let item = state.downloads[index].clone();
        let proxy = resolve_proxy(&state, &item.source.proxy)?;
        let destination = self.destination_dir(&item)?;
        drop(state);
        {
            let mut state = self.state.write().await;
            let current = find_download_mut(&mut state, &item.id)?;
            if !matches!(&current.state, DownloadState::Queued) {
                return Ok(None);
            }
            current.state = DownloadState::Downloading { speed_bytes: 0 };
            current.updated_at = Utc::now();
            state.revision += 1;
        }
        if let Err(error) = self.commit().await {
            let revision = {
                let mut state = self.state.write().await;
                let current = find_download_mut(&mut state, &item.id)?;
                current.state = DownloadState::Failed {
                    message: error.to_string(),
                    recoverable: false,
                };
                current.updated_at = Utc::now();
                state.revision += 1;
                state.revision
            };
            self.emit_revision(revision);
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
        if self.tasks.lock().await.contains_key(&id) {
            return;
        }
        let cancellation = CancellationToken::new();
        let gate = Arc::new(Notify::new());
        let manager = Arc::clone(self);
        let task_cancellation = cancellation.clone();
        let task_gate = Arc::clone(&gate);
        let task_id = id.clone();
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
            manager.tasks.lock().await.remove(&task_id);
            manager.scheduler.notify_one();
        });
        let replaced = self
            .tasks
            .lock()
            .await
            .insert(id, TaskControl { cancellation, join });
        debug_assert!(replaced.is_none());
        gate.notify_one();
    }

    async fn run_job(&self, id: String, input: EngineInput, cancellation: CancellationToken) {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let transfer = DownloadEngine::run(input, cancellation, sender);
        tokio::pin!(transfer);

        let result: Result<_, JobFailure> = loop {
            tokio::select! {
                progress = receiver.recv() => {
                    if let Some(progress) = progress {
                        if let Err(error) = self.update_progress(&id, progress).await {
                            break Err(JobFailure::Persistence(error));
                        }
                    }
                }
                result = &mut transfer => break result.map_err(JobFailure::Engine),
            }
        };

        match result {
            Ok(output) => {
                let mut state = self.state.write().await;
                if let Ok(item) = find_download_mut(&mut state, &id) {
                    item.transfer.downloaded_bytes = output.downloaded_bytes;
                    item.transfer.size = output.size;
                    item.transfer.validator = output.validator;
                    item.state = DownloadState::Completed {
                        completed_at: Utc::now(),
                    };
                    item.updated_at = Utc::now();
                    state.revision += 1;
                }
            }
            Err(JobFailure::Engine(EngineError::Cancelled)) => return,
            Err(failure) => {
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
                        item.updated_at = Utc::now();
                        state.revision += 1;
                    }
                }
            }
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
        let (revision, identity_changed) = {
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
            item.state = DownloadState::Downloading {
                speed_bytes: progress.speed_bytes,
            };
            item.updated_at = Utc::now();
            state.revision += 1;
            (state.revision, identity_changed)
        };
        self.emit_revision(revision);
        if identity_changed || revision % 4 == 0 {
            self.persist().await?;
        }
        Ok(())
    }

    async fn stop_task(&self, id: &str) {
        let control = { self.tasks.lock().await.remove(id) };
        if let Some(control) = control {
            self.stopping.lock().await.insert(id.to_owned());
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
                set_private_permissions(&reservation).await?;
                file.write_all(item.id.as_bytes()).await?;
                file.sync_all().await?;
                Ok(())
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
        let temporary = self
            .store_path
            .with_extension(format!("json.{}.tmp", snapshot.revision));
        fs::write(&temporary, bytes).await?;
        set_private_permissions(&temporary).await?;
        let file = fs::OpenOptions::new().read(true).open(&temporary).await?;
        file.sync_all().await?;
        if let Err(error) = fs::rename(&temporary, &self.store_path).await {
            if !fs::try_exists(&self.store_path).await? {
                return Err(error.into());
            }
            let backup = self.store_path.with_extension("json.bak");
            remove_if_exists(&backup).await?;
            fs::rename(&self.store_path, &backup).await?;
            if let Err(replace_error) = fs::rename(&temporary, &self.store_path).await {
                let _ = fs::rename(&backup, &self.store_path).await;
                return Err(replace_error.into());
            }
            remove_if_exists(&backup).await?;
        }
        sync_parent_directory(&self.store_path).await?;
        Ok(())
    }

    fn emit_revision(&self, revision: u64) {
        let _ = self
            .app
            .emit("downloads://changed", RevisionEvent { revision });
    }
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
        is_safe_configured_directory, is_safe_relative_subdirectory, is_windows_reserved_name,
    };

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
}
