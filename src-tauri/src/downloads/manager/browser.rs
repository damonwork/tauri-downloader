use super::super::model::{
    AppSettings, BrowserDownloadInput, CreateDownloadInput, DownloadCategory, DownloadSource,
    HeaderEntry, ProxySelection,
};
use super::validation::{needs_remote_file_name, sanitize_detected_file_name};

pub(super) fn create_input(
    input: BrowserDownloadInput,
    settings: &AppSettings,
) -> CreateDownloadInput {
    let detected_file_name = input
        .file_name
        .as_deref()
        .and_then(sanitize_detected_file_name);
    let file_name = detected_file_name
        .as_deref()
        .filter(|name| !needs_remote_file_name(name))
        .map(str::to_owned)
        .or_else(|| page_file_name(&input, detected_file_name.as_deref()))
        .or(detected_file_name)
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

fn page_file_name(input: &BrowserDownloadInput, fallback: Option<&str>) -> Option<String> {
    let title = input.page_title.as_deref()?.trim();
    let (anime, episode) = episode_from_title(title)?;
    let extension = fallback
        .and_then(|value| value.rsplit_once('.'))
        .map(|(_, extension)| extension)
        .filter(|extension| !needs_remote_file_name(&format!("file.{extension}")))
        .unwrap_or("mp4");
    sanitize_detected_file_name(&format!("{anime} Episodio {episode}.{extension}"))
}

fn episode_from_title(title: &str) -> Option<(String, String)> {
    let lower = title.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("ver episodio ") {
        let episode_start = "ver episodio ".len();
        let separator = rest.find(" de ")? + episode_start;
        let episode = title[episode_start..separator].trim();
        let anime_start = separator + " de ".len();
        let anime = strip_fansub(&title[anime_start..]);
        if !episode.is_empty() && !anime.is_empty() {
            return Some((anime.to_owned(), episode.to_owned()));
        }
    }
    if let Some(separator) = lower.find(" episodio ") {
        let anime = title[..separator].trim();
        let rest = &lower[separator + " episodio ".len()..];
        let episode: String = rest
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect();
        if !anime.is_empty() && !episode.is_empty() {
            return Some((anime.to_owned(), episode));
        }
    }
    None
}

fn strip_fansub(value: &str) -> &str {
    value
        .split_once(" - ")
        .map_or(value, |(name, _)| name)
        .trim()
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
                page_title: None,
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

    #[test]
    fn browser_input_uses_episode_page_title_for_technical_names() {
        let input = create_input(
            BrowserDownloadInput {
                url: "https://re.animepelix.net/redirect.php?id=...".to_owned(),
                file_name: Some("redirect.php".to_owned()),
                page_url: Some("https://animefenix2.tv/ver/youjo-senki-s2-6".to_owned()),
                page_title: Some("Ver episodio 6 de Youjo Senki II - MonosChinos".to_owned()),
                referrer: None,
                user_agent: None,
                cookies: Vec::new(),
                force_single_stream: true,
            },
            &AppSettings::default(),
        );

        assert_eq!(input.file_name, "Youjo Senki II Episodio 6.mp4");
    }

    #[test]
    fn page_file_name_matches_episode_title_patterns() {
        let from_title = |page_title: &str| {
            create_input(
                BrowserDownloadInput {
                    url: "https://site.example/hugging.php".to_owned(),
                    file_name: Some("hugging.php".to_owned()),
                    page_url: Some("https://site.example/ver/one-piece-1050".to_owned()),
                    page_title: Some(page_title.to_owned()),
                    referrer: None,
                    user_agent: None,
                    cookies: Vec::new(),
                    force_single_stream: true,
                },
                &AppSettings::default(),
            )
            .file_name
        };
        assert_eq!(
            from_title("Ver episodio 6 de Youjo Senki II - MonosChinos"),
            "Youjo Senki II Episodio 6.mp4"
        );
        assert_eq!(
            from_title("One Piece Episodio 1050 - Subs"),
            "One Piece Episodio 1050.mp4"
        );
        assert_eq!(
            from_title("Attack on Titan Episodio 12 (1080p)"),
            "Attack on Titan Episodio 12.mp4"
        );
        assert_eq!(
            from_title("Boku no Hero Academia Episodio 3 v2"),
            "Boku no Hero Academia Episodio 3.mp4"
        );
        assert_eq!(from_title("Guía de descargas y tutoriales"), "hugging.php");
    }
}
