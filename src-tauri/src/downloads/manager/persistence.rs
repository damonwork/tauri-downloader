use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use chrono::Utc;
use tauri::{Emitter, Manager};
use tokio::{fs, io::AsyncWriteExt, sync::Mutex};

use super::files::{remove_if_exists, set_private_permissions, sync_parent_directory};
use super::validation::{find_download_mut, reservation_path};
use super::{
    AppError, AppSnapshot, DownloadItem, DownloadManager, DownloadProgressEvent, DownloadState,
    RevisionEvent, TaskControl, PROGRESS_PERSIST_INTERVAL,
};

impl DownloadManager {
    pub(super) async fn operation_lock(&self, id: &str) -> Arc<Mutex<()>> {
        let mut operations = self.operation_locks.lock().await;
        Arc::clone(
            operations
                .entry(id.to_owned())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    pub(super) async fn stop_task(&self, id: &str) {
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
        if let Some(TaskControl {
            cancellation, join, ..
        }) = control
        {
            cancellation.cancel();
            let _ = join.await;
            self.stopping.lock().await.remove(id);
        }
    }

    pub(super) fn destination_dir(&self, item: &DownloadItem) -> Result<PathBuf, AppError> {
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

    pub(super) async fn reserve_output(&self, item: &DownloadItem) -> Result<(), AppError> {
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

    pub(super) async fn release_output(&self, item: &DownloadItem) -> Result<(), AppError> {
        let directory = self.destination_dir(item)?;
        let reservation = reservation_path(&directory, &item.file_name);
        match fs::read_to_string(&reservation).await {
            Ok(owner) if owner == item.id => remove_if_exists(&reservation).await,
            Ok(_) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub(super) async fn restore_output_reservations(&self) {
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

    pub(super) async fn commit(&self) -> Result<(), AppError> {
        self.persist().await?;
        let revision = self.state.read().await.revision;
        self.emit_revision(revision);
        Ok(())
    }

    pub(super) async fn persist(&self) -> Result<(), AppError> {
        let _guard = self.persistence.lock().await;
        let snapshot = self.state.read().await.clone();
        let bytes = serde_json::to_vec_pretty(&snapshot)?;
        persist_snapshot(&self.store_path, &bytes, snapshot.revision).await
    }

    pub(super) fn emit_revision(&self, revision: u64) {
        let _ = self
            .app
            .emit("downloads://changed", RevisionEvent { revision });
    }

    pub(super) fn emit_progress(&self, event: DownloadProgressEvent) {
        let _ = self.app.emit("downloads://progress", event);
    }

    pub(super) async fn should_persist_progress(&self, id: &str, force: bool) -> bool {
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

pub(super) async fn persist_snapshot(
    path: &Path,
    bytes: &[u8],
    revision: u64,
) -> Result<(), AppError> {
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

pub(super) async fn load_snapshot(path: &Path) -> Result<AppSnapshot, AppError> {
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
