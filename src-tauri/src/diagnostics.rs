use std::{collections::BTreeMap, io, path::PathBuf, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::sync::{Mutex, RwLock};

const DIAGNOSTICS_FILE: &str = "diagnostics.json";
const MAX_DIAGNOSTIC_ENTRIES: usize = 500;
const MAX_DIAGNOSTIC_BYTES: usize = 256 * 1024;
const PERSIST_DELAY: std::time::Duration = std::time::Duration::from_millis(250);
const MAX_DETAIL_VALUE_CHARS: usize = 240;

pub type DiagnosticDetails = BTreeMap<String, String>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticLevel {
    Debug,
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEntry {
    pub id: String,
    pub at: DateTime<Utc>,
    pub level: DiagnosticLevel,
    pub scope: String,
    pub event: String,
    pub message: String,
    pub details: DiagnosticDetails,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticSnapshot {
    pub entries: Vec<DiagnosticEntry>,
    pub max_entries: usize,
}

pub struct Diagnostics {
    entries: RwLock<Vec<DiagnosticEntry>>,
    persistence: Mutex<()>,
    path: PathBuf,
    dirty: Arc<std::sync::atomic::AtomicBool>,
}

impl Diagnostics {
    pub async fn load(app: &AppHandle) -> io::Result<Arc<Self>> {
        let directory = app
            .path()
            .app_data_dir()
            .map_err(|error| io::Error::other(error.to_string()))?;
        tokio::fs::create_dir_all(&directory).await?;
        let path = directory.join(DIAGNOSTICS_FILE);
        let mut entries = match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice::<Vec<DiagnosticEntry>>(&bytes).unwrap_or_default(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        entries.truncate(MAX_DIAGNOSTIC_ENTRIES);
        let diagnostics = Arc::new(Self {
            entries: RwLock::new(entries),
            persistence: Mutex::new(()),
            path,
            dirty: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        });
        diagnostics.spawn_persister();
        Ok(diagnostics)
    }

    fn spawn_persister(self: &Arc<Self>) {
        let diagnostics = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let mut delay = PERSIST_DELAY;
            loop {
                tokio::time::sleep(delay).await;
                if diagnostics
                    .dirty
                    .swap(false, std::sync::atomic::Ordering::Relaxed)
                {
                    if let Err(error) = diagnostics.persist().await {
                        eprintln!("Fluxor diagnostics persistence failed: {error}");
                        diagnostics
                            .dirty
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                        delay = (delay * 2).min(std::time::Duration::from_secs(30));
                        continue;
                    }
                }
                delay = PERSIST_DELAY;
            }
        });
    }

    pub async fn snapshot(&self) -> DiagnosticSnapshot {
        DiagnosticSnapshot {
            entries: self.entries.read().await.clone(),
            max_entries: MAX_DIAGNOSTIC_ENTRIES,
        }
    }

    pub async fn clear(&self) -> io::Result<()> {
        self.entries.write().await.clear();
        self.persist().await
    }

    pub async fn record(
        &self,
        level: DiagnosticLevel,
        scope: &str,
        event: &str,
        message: impl Into<String>,
        details: DiagnosticDetails,
    ) {
        let entry = DiagnosticEntry {
            id: uuid::Uuid::new_v4().to_string(),
            at: Utc::now(),
            level,
            scope: scope.to_owned(),
            event: event.to_owned(),
            message: message.into(),
            details,
        };
        {
            let mut entries = self.entries.write().await;
            entries.insert(0, entry);
            entries.truncate(MAX_DIAGNOSTIC_ENTRIES);
        }
        self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    async fn persist(&self) -> io::Result<()> {
        let _guard = self.persistence.lock().await;
        let bytes = self.encoded_bytes().await;
        atomic_write(&self.path, &bytes).await
    }

    async fn encoded_bytes(&self) -> Vec<u8> {
        let mut entries = self.entries.write().await;
        let mut bytes = serde_json::to_vec(&*entries).unwrap_or_default();
        if bytes.len() > MAX_DIAGNOSTIC_BYTES {
            let mut total = 0_usize;
            let mut kept = Vec::new();
            for entry in entries.drain(..) {
                total += entry_message_size(&entry);
                if total > MAX_DIAGNOSTIC_BYTES && !kept.is_empty() {
                    break;
                }
                kept.push(entry);
            }
            *entries = kept;
            bytes = serde_json::to_vec(&*entries).unwrap_or_default();
        }
        bytes
    }
}

fn entry_message_size(entry: &DiagnosticEntry) -> usize {
    entry.message.len()
        + entry.scope.len()
        + entry.event.len()
        + entry
            .details
            .values()
            .map(|value| value.len().min(MAX_DETAIL_VALUE_CHARS))
            .sum::<usize>()
}

async fn atomic_write(path: &PathBuf, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension("json.tmp");
    tokio::fs::write(&temporary, bytes).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600)).await?;
    }
    tokio::fs::rename(&temporary, path).await
}

pub fn diagnostic_details<const N: usize>(values: [(&str, String); N]) -> DiagnosticDetails {
    values
        .into_iter()
        .map(|(name, value)| {
            (
                name.to_owned(),
                value.chars().take(MAX_DETAIL_VALUE_CHARS).collect(),
            )
        })
        .collect()
}

pub fn safe_url(value: &str) -> String {
    reqwest::Url::parse(value).map_or_else(
        |_| "URL no válida".to_owned(),
        |url| {
            format!(
                "{}://{}{}",
                url.scheme(),
                url.host_str().unwrap_or_default(),
                url.path()
            )
        },
    )
}
