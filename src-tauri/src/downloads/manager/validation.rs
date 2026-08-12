mod configuration;
mod downloads;

pub(super) use configuration::{
    destination_for_category, reservation_path, resolve_proxy, validate_proxy, validate_settings,
};
pub(super) use downloads::{
    category_for_file, find_download, find_download_mut, needs_remote_file_name,
    sanitize_detected_file_name, validate_create_input, validate_source,
};

#[cfg(test)]
pub(super) use configuration::{is_safe_configured_directory, is_safe_relative_subdirectory};
#[cfg(test)]
pub(super) use downloads::{is_engine_controlled_header, is_windows_reserved_name};
