use std::path::{Component, Path};

use super::super::{
    AppError, AppSettings, AppSnapshot, DownloadCategory, ProxyProfile, ProxySelection,
    ResolvedProxy, MAX_SPEED_LIMIT_BYTES, MAX_THREADS_PER_DOWNLOAD,
};

pub(in super::super) fn resolve_proxy(
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

pub(in super::super) fn destination_for_category(
    settings: &AppSettings,
    category: &DownloadCategory,
) -> String {
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

pub(in super::super) fn reservation_path(directory: &Path, file_name: &str) -> std::path::PathBuf {
    directory.join(format!(".{file_name}.fluxor.lock"))
}

pub(in super::super) fn validate_settings(settings: &AppSettings) -> Result<(), AppError> {
    if settings.max_concurrent == 0
        || settings.max_concurrent > super::super::MAX_CONCURRENT_DOWNLOADS
    {
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

pub(in super::super) fn is_safe_configured_directory(value: &str) -> bool {
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

pub(in super::super) fn is_safe_relative_subdirectory(value: &str) -> bool {
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

pub(in super::super) fn validate_proxy(proxy: &ProxyProfile) -> Result<(), AppError> {
    if proxy.id.trim().is_empty() || proxy.name.trim().is_empty() {
        return Err(AppError::Validation(
            "El proxy necesita nombre e identificador".to_owned(),
        ));
    }
    reqwest::Proxy::all(&proxy.url)
        .map_err(|_| AppError::Validation("La URL del proxy no es válida".to_owned()))?;
    Ok(())
}
