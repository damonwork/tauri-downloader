use super::persistence::persist_snapshot;
use super::validation::{
    destination_for_category, is_engine_controlled_header, is_safe_configured_directory,
    is_safe_relative_subdirectory, is_windows_reserved_name, needs_remote_file_name,
    sanitize_detected_file_name,
};
use super::MAX_FILE_NAME_UTF16_UNITS;
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
