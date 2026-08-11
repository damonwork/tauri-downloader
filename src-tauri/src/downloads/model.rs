use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderEntry {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieEntry {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ProxySelection {
    Direct,
    Profile { profile_id: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSource {
    pub url: String,
    pub headers: Vec<HeaderEntry>,
    pub cookies: Vec<CookieEntry>,
    pub proxy: ProxySelection,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadCategory {
    Video,
    Archive,
    Document,
    Audio,
    Other,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TransferSize {
    Unknown,
    Known { total_bytes: u64 },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SourceValidator {
    None,
    Etag { value: String },
    LastModified { value: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub downloaded_bytes: u64,
    pub size: TransferSize,
    pub validator: SourceValidator,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DownloadState {
    Queued,
    Downloading { speed_bytes: u64 },
    Paused,
    Completed { completed_at: DateTime<Utc> },
    Failed { message: String, recoverable: bool },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadItem {
    pub id: String,
    pub file_name: String,
    pub category: DownloadCategory,
    pub state: DownloadState,
    pub source: DownloadSource,
    pub destination: String,
    pub transfer: TransferProgress,
    pub threads: u8,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDownloadInput {
    pub source: DownloadSource,
    pub file_name: String,
    pub category: DownloadCategory,
    pub destination: String,
    pub threads: u8,
    pub start_immediately: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadAction {
    Pause,
    Resume,
    Retry,
    Restart,
    Remove,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ProxyHealth {
    Untested,
    Checking,
    Online { latency_ms: u64 },
    Offline { reason: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyProfile {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub health: ProxyHealth,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDirectories {
    pub video: String,
    pub archive: String,
    pub document: String,
    pub audio: String,
    pub other: String,
}

impl Default for CategoryDirectories {
    fn default() -> Self {
        Self {
            video: "Videos".to_owned(),
            archive: "Comprimidos".to_owned(),
            document: "Documentos".to_owned(),
            audio: "Audio".to_owned(),
            other: "Otros".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub max_concurrent: u8,
    pub default_threads: u8,
    pub download_directory: String,
    pub organize_by_category: bool,
    pub category_directories: CategoryDirectories,
    pub start_immediately: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            max_concurrent: 3,
            default_threads: 8,
            download_directory: "Fluxor".to_owned(),
            organize_by_category: true,
            category_directories: CategoryDirectories::default(),
            start_immediately: true,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub revision: u64,
    pub downloads: Vec<DownloadItem>,
    pub proxies: Vec<ProxyProfile>,
    pub settings: AppSettings,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionEvent {
    pub revision: u64,
}
