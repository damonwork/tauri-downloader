use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager};
use tokio::sync::{Mutex, Notify, RwLock};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    engine::{EngineError, ResolvedProxy},
    error::AppError,
    model::{
        AppSettings, AppSnapshot, BrowserDownloadInput, CreateDownloadInput, DownloadAction,
        DownloadCategory, DownloadItem, DownloadProgressEvent, DownloadSource, DownloadState,
        ProxyHealth, ProxyProfile, ProxySelection, ResumeSupport, RevisionEvent, SegmentState,
        SourceValidator, TransferPhase, TransferProgress, TransferSize, TransferTelemetry,
    },
};
mod browser;
mod controls;
mod files;
mod jobs;
mod persistence;
mod progress;
mod validation;

use files::reset_live_telemetry;
use persistence::load_snapshot;
use validation::{
    category_for_file, destination_for_category, find_download, find_download_mut,
    needs_remote_file_name, resolve_proxy, sanitize_detected_file_name, validate_create_input,
    validate_proxy, validate_settings, validate_source,
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
    speed_limit: Arc<std::sync::atomic::AtomicU64>,
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
        tokio::fs::create_dir_all(&app_dir).await?;
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

    pub async fn reveal_download(&self, id: &str) -> Result<(), AppError> {
        let item = {
            let state = self.state.read().await;
            let item = find_download(&state, id)?;
            if !matches!(&item.state, DownloadState::Completed { .. }) {
                return Err(AppError::Validation(
                    "Solo puedes abrir la ubicación de una descarga completada".to_owned(),
                ));
            }
            item.clone()
        };
        let path = self.destination_dir(&item)?.join(&item.file_name);
        reveal_path(&path)
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
        if !input.source.force_single_stream
            && !input.file_name_customized
            && needs_remote_file_name(&input.file_name)
        {
            if let Some(detected) =
                super::engine::DownloadEngine::detect_file_name(&input.source, &proxy).await
            {
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

    pub async fn add_from_browser(
        &self,
        input: BrowserDownloadInput,
    ) -> Result<DownloadItem, AppError> {
        let settings = self.state.read().await.settings.clone();
        self.add(browser::create_input(input, &settings)).await
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

    pub async fn set_speed_limit(&self, id: &str, bytes_per_second: u64) -> Result<(), AppError> {
        if bytes_per_second > MAX_SPEED_LIMIT_BYTES {
            return Err(AppError::Validation(
                "El límite de velocidad supera el máximo permitido".to_owned(),
            ));
        }
        let operation = self.operation_lock(id).await;
        let _guard = operation.lock().await;
        let previous = {
            let mut state = self.state.write().await;
            let item = find_download_mut(&mut state, id)?;
            if matches!(&item.state, DownloadState::Completed { .. }) {
                return Err(AppError::Validation(
                    "La descarga ya se completó; no puede ajustarse su límite".to_owned(),
                ));
            }
            let previous = item.speed_limit_bytes;
            item.speed_limit_bytes = bytes_per_second;
            item.updated_at = Utc::now();
            state.revision += 1;
            previous
        };
        if let Some(control) = self.tasks.lock().await.get(id) {
            control
                .speed_limit
                .store(bytes_per_second, std::sync::atomic::Ordering::Relaxed);
        }
        if let Err(error) = self.commit().await {
            let mut state = self.state.write().await;
            if let Ok(item) = find_download_mut(&mut state, id) {
                item.speed_limit_bytes = previous;
                item.updated_at = Utc::now();
                state.revision += 1;
            }
            if let Some(control) = self.tasks.lock().await.get(id) {
                control
                    .speed_limit
                    .store(previous, std::sync::atomic::Ordering::Relaxed);
            }
            return Err(error);
        }
        Ok(())
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
}

fn reveal_path(path: &std::path::Path) -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    let result = reveal_path_windows(path);
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(path))
        .spawn();

    result
        .map(|_| ())
        .map_err(|error| AppError::ExternalOpen(error.to_string()))
}

#[cfg(target_os = "windows")]
fn reveal_path_windows(path: &std::path::Path) -> std::io::Result<std::process::Child> {
    let select = format!("/select,\"{}\"", path.display());
    match std::process::Command::new("explorer.exe")
        .arg(&select)
        .spawn()
    {
        Ok(child) => Ok(child),
        Err(error) => std::process::Command::new("explorer.exe")
            .arg(path.parent().unwrap_or(path))
            .spawn()
            .map_err(|_| error),
    }
}

#[cfg(test)]
mod tests;
