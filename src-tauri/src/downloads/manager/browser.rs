use super::super::model::{
    AppSettings, BrowserDownloadInput, CreateDownloadInput, DownloadCategory, DownloadSource,
    HeaderEntry, ProxySelection,
};
use super::validation::sanitize_detected_file_name;

pub(super) fn create_input(
    input: BrowserDownloadInput,
    settings: &AppSettings,
) -> CreateDownloadInput {
    let file_name = input
        .file_name
        .as_deref()
        .and_then(sanitize_detected_file_name)
        .unwrap_or_else(|| "download".to_owned());
    let mut headers = Vec::new();
    if let Some(user_agent) = nonempty(input.user_agent) {
        headers.push(HeaderEntry {
            name: "User-Agent".to_owned(),
            value: user_agent,
        });
    }
    if let Some(referer) = nonempty(input.referrer).or_else(|| nonempty(input.page_url)) {
        headers.push(HeaderEntry {
            name: "Referer".to_owned(),
            value: referer,
        });
    }
    CreateDownloadInput {
        source: DownloadSource {
            url: input.url,
            headers,
            cookies: input.cookies,
            proxy: ProxySelection::Direct,
            force_single_stream: input.force_single_stream,
        },
        file_name,
        file_name_customized: false,
        category: DownloadCategory::Other,
        category_customized: false,
        destination: settings.download_directory.clone(),
        destination_customized: false,
        threads: settings.default_threads,
        speed_limit_bytes: settings.default_speed_limit_bytes,
        start_immediately: settings.start_immediately,
    }
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::downloads::model::CookieEntry;

    #[test]
    fn browser_input_maps_safe_request_metadata_and_defaults() {
        let input = create_input(
            BrowserDownloadInput {
                url: "https://media.example/video.mp4".to_owned(),
                file_name: Some("episode.mp4".to_owned()),
                page_url: Some("https://site.example/watch".to_owned()),
                referrer: None,
                user_agent: Some("Browser".to_owned()),
                cookies: vec![CookieEntry {
                    name: "session".to_owned(),
                    value: "secret".to_owned(),
                }],
                force_single_stream: true,
            },
            &AppSettings::default(),
        );

        assert_eq!(input.file_name, "episode.mp4");
        assert_eq!(input.source.headers.len(), 2);
        assert_eq!(input.source.cookies.len(), 1);
        assert!(input.source.force_single_stream);
        assert!(!input.destination_customized);
    }
}
