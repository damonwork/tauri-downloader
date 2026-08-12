use std::path::{Component, Path};

use super::{
    AppError, AppSettings, AppSnapshot, CreateDownloadInput, DownloadCategory, DownloadItem,
    DownloadSource, ProxyProfile, ProxySelection, ResolvedProxy, MAX_FILE_NAME_UTF16_UNITS,
    MAX_SPEED_LIMIT_BYTES, MAX_THREADS_PER_DOWNLOAD,
};

pub(super) fn find_download<'a>(
    state: &'a AppSnapshot,
    id: &str,
) -> Result<&'a DownloadItem, AppError> {
    state
        .downloads
        .iter()
        .find(|item| item.id == id)
        .ok_or(AppError::DownloadNotFound)
}

pub(super) fn find_download_mut<'a>(
    state: &'a mut AppSnapshot,
    id: &str,
) -> Result<&'a mut DownloadItem, AppError> {
    state
        .downloads
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or(AppError::DownloadNotFound)
}

pub(super) fn resolve_proxy(
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

pub(super) fn validate_create_input(input: &CreateDownloadInput) -> Result<(), AppError> {
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

pub(super) fn is_windows_reserved_name(file_name: &str) -> bool {
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

pub(super) fn sanitize_detected_file_name(value: &str) -> Option<String> {
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

pub(super) fn needs_remote_file_name(file_name: &str) -> bool {
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

pub(super) fn category_for_file(file_name: &str) -> DownloadCategory {
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

pub(super) fn destination_for_category(
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

pub(super) fn reservation_path(directory: &Path, file_name: &str) -> std::path::PathBuf {
    directory.join(format!(".{file_name}.fluxor.lock"))
}

pub(super) fn validate_source(source: &DownloadSource) -> Result<(), AppError> {
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

pub(super) fn is_engine_controlled_header(name: &str) -> bool {
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

pub(super) fn validate_settings(settings: &AppSettings) -> Result<(), AppError> {
    if settings.max_concurrent == 0 || settings.max_concurrent > super::MAX_CONCURRENT_DOWNLOADS {
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

pub(super) fn is_safe_configured_directory(value: &str) -> bool {
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

pub(super) fn is_safe_relative_subdirectory(value: &str) -> bool {
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

pub(super) fn validate_proxy(proxy: &ProxyProfile) -> Result<(), AppError> {
    if proxy.id.trim().is_empty() || proxy.name.trim().is_empty() {
        return Err(AppError::Validation(
            "El proxy necesita nombre e identificador".to_owned(),
        ));
    }
    reqwest::Proxy::all(&proxy.url)
        .map_err(|_| AppError::Validation("La URL del proxy no es válida".to_owned()))?;
    Ok(())
}
