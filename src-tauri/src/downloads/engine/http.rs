use std::time::Duration;

use reqwest::{
    header::{
        HeaderMap, HeaderName, HeaderValue, ACCEPT_ENCODING, ACCEPT_RANGES, CONTENT_DISPOSITION,
        CONTENT_LENGTH, CONTENT_RANGE, COOKIE, ETAG, IF_RANGE, LAST_MODIFIED, RANGE,
    },
    redirect::Policy,
    Client, RequestBuilder, StatusCode,
};
use tokio::time;
use tokio_util::sync::CancellationToken;

use super::{EngineError, ResolvedProxy, DEFAULT_USER_AGENT, MAX_REDIRECTS, METADATA_TIMEOUT};
use crate::downloads::model::{DownloadSource, ResumeSupport, SourceValidator, TransferSize};

#[derive(Clone)]
pub(super) struct ProbeResult {
    pub(super) size: TransferSize,
    pub(super) validator: SourceValidator,
    pub(super) accepts_ranges: bool,
}

pub(super) async fn probe_source(
    client: &Client,
    source: &DownloadSource,
    cancellation: &CancellationToken,
) -> Result<Option<ProbeResult>, EngineError> {
    let request = apply_source_headers(client.head(&source.url), source)?;
    let response = tokio::select! {
        biased;
        _ = cancellation.cancelled() => return Err(EngineError::Cancelled),
        response = request.send() => match response {
            Ok(response) => response,
            Err(_) => return Ok(None),
        },
    };
    if !response.status().is_success() {
        return Ok(None);
    }
    let size = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(TransferSize::Unknown, |total_bytes| TransferSize::Known {
            total_bytes,
        });
    let accepts_ranges = accepts_ranges(response.headers());
    Ok(Some(ProbeResult {
        size,
        validator: response_validator(response.headers()),
        accepts_ranges,
    }))
}

pub(super) async fn confirm_segment_source(
    client: &Client,
    source: &DownloadSource,
    mut probe: ProbeResult,
    cancellation: &CancellationToken,
) -> Result<ProbeResult, EngineError> {
    let TransferSize::Known { total_bytes } = &probe.size else {
        return Err(EngineError::InvalidContentRange);
    };
    let mut request =
        apply_source_headers(client.get(&source.url), source)?.header(RANGE, "bytes=0-0");
    request = apply_if_range(request, &probe.validator)?;
    let mut response = tokio::select! {
        biased;
        _ = cancellation.cancelled() => return Err(EngineError::Cancelled),
        response = request.send() => response.map_err(|_| EngineError::Request)?,
    };
    if response.status() != StatusCode::PARTIAL_CONTENT {
        return Err(EngineError::ResumeRejected);
    }
    validate_content_range(response.headers(), 0, Some(0), Some(*total_bytes))?;
    probe.validator =
        confirmed_segment_validator(&probe.validator, response_validator(response.headers()))?;
    let first_chunk = tokio::select! {
        biased;
        _ = cancellation.cancelled() => return Err(EngineError::Cancelled),
        chunk = response.chunk() => chunk.map_err(|_| EngineError::Request)?,
    };
    if first_chunk.as_ref().is_none_or(|chunk| chunk.len() != 1) {
        return Err(EngineError::InvalidContentRange);
    }
    let extra_chunk = tokio::select! {
        biased;
        _ = cancellation.cancelled() => return Err(EngineError::Cancelled),
        chunk = response.chunk() => chunk.map_err(|_| EngineError::Request)?,
    };
    if extra_chunk.is_some() {
        return Err(EngineError::InvalidContentRange);
    }
    Ok(probe)
}

pub(super) fn build_client(
    proxy: &ResolvedProxy,
    source: &DownloadSource,
) -> Result<Client, EngineError> {
    let carries_credentials = !source.headers.is_empty() || !source.cookies.is_empty();
    let mut default_headers = HeaderMap::new();
    default_headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    let mut builder = Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .default_headers(default_headers)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(45))
        .redirect(if carries_credentials {
            Policy::none()
        } else {
            Policy::limited(MAX_REDIRECTS)
        });
    match proxy {
        ResolvedProxy::Direct => builder = builder.no_proxy(),
        ResolvedProxy::Url(url) => {
            let proxy = reqwest::Proxy::all(url).map_err(|_| EngineError::Request)?;
            builder = builder.proxy(proxy);
        }
    }
    builder.build().map_err(|_| EngineError::Request)
}

pub(super) fn apply_source_headers(
    mut request: RequestBuilder,
    source: &DownloadSource,
) -> Result<RequestBuilder, EngineError> {
    for header in &source.headers {
        let name = HeaderName::from_bytes(header.name.as_bytes())
            .map_err(|_| EngineError::InvalidHeader(header.name.clone()))?;
        if name == RANGE
            || name == IF_RANGE
            || name == CONTENT_LENGTH
            || name == CONTENT_RANGE
            || name == COOKIE
            || matches!(
                name.as_str(),
                "accept-encoding"
                    | "connection"
                    | "host"
                    | "te"
                    | "trailer"
                    | "transfer-encoding"
                    | "upgrade"
            )
        {
            return Err(EngineError::InvalidHeader(header.name.clone()));
        }
        let value = HeaderValue::from_str(&header.value)
            .map_err(|_| EngineError::InvalidHeader(header.name.clone()))?;
        request = request.header(name, value);
    }
    if !source.cookies.is_empty() {
        let value = source
            .cookies
            .iter()
            .map(|cookie| format!("{}={}", cookie.name, cookie.value))
            .collect::<Vec<_>>()
            .join("; ");
        request = request.header(
            COOKIE,
            HeaderValue::from_str(&value)
                .map_err(|_| EngineError::InvalidHeader("Cookie".to_owned()))?,
        );
    }
    Ok(request)
}

pub(super) fn apply_if_range(
    request: RequestBuilder,
    validator: &SourceValidator,
) -> Result<RequestBuilder, EngineError> {
    let value = match validator {
        SourceValidator::None => return Ok(request),
        SourceValidator::Etag { value } | SourceValidator::LastModified { value } => value,
    };
    let header = HeaderValue::from_str(value)
        .map_err(|_| EngineError::InvalidHeader("If-Range".to_owned()))?;
    Ok(request.header(IF_RANGE, header))
}

pub(super) fn response_validator(headers: &HeaderMap) -> SourceValidator {
    if let Some(value) = headers.get(ETAG).and_then(|value| value.to_str().ok()) {
        if !value.trim_start().starts_with("W/") {
            return SourceValidator::Etag {
                value: value.to_owned(),
            };
        }
    }
    if let Some(value) = headers
        .get(LAST_MODIFIED)
        .and_then(|value| value.to_str().ok())
    {
        return SourceValidator::LastModified {
            value: value.to_owned(),
        };
    }
    SourceValidator::None
}

pub(super) fn confirmed_segment_validator(
    expected: &SourceValidator,
    observed: SourceValidator,
) -> Result<SourceValidator, EngineError> {
    if matches!(&observed, SourceValidator::None) {
        return Ok(expected.clone());
    }
    if matches!(expected, SourceValidator::None) || expected == &observed {
        return Ok(observed);
    }
    Err(EngineError::SourceChanged)
}

pub(super) fn validate_segment_response_validator(
    expected: &SourceValidator,
    observed: &SourceValidator,
) -> Result<(), EngineError> {
    if matches!(observed, SourceValidator::None) || expected == observed {
        return Ok(());
    }
    Err(EngineError::SourceChanged)
}

pub(super) fn accepts_ranges(headers: &HeaderMap) -> bool {
    headers
        .get(ACCEPT_RANGES)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("bytes"))
}

pub(super) fn resume_support(accepts_ranges: bool) -> ResumeSupport {
    if !accepts_ranges {
        ResumeSupport::Unsupported {
            reason: "El servidor no acepta solicitudes por rango".to_owned(),
        }
    } else {
        ResumeSupport::Supported
    }
}

pub(super) fn content_disposition_file_name(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(CONTENT_DISPOSITION)?.to_str().ok()?;
    let mut regular = None;
    for parameter in split_header_parameters(value).into_iter().skip(1) {
        let Some((name, raw_value)) = parameter.split_once('=') else {
            continue;
        };
        let name = name.trim();
        let raw_value = unquote_header_value(raw_value.trim());
        if name.eq_ignore_ascii_case("filename*") {
            if let Some((charset, encoded)) = raw_value.split_once("''") {
                if charset.eq_ignore_ascii_case("UTF-8") {
                    if let Some(decoded) = percent_decode_utf8(encoded) {
                        return Some(decoded);
                    }
                }
            }
        } else if name.eq_ignore_ascii_case("filename") && regular.is_none() {
            regular = Some(raw_value);
        }
    }
    regular.filter(|value| !value.is_empty())
}

fn split_header_parameters(value: &str) -> Vec<String> {
    let mut parameters = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            current.push(character);
            escaped = false;
        } else if character == '\\' && quoted {
            current.push(character);
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
            current.push(character);
        } else if character == ';' && !quoted {
            parameters.push(current.trim().to_owned());
            current.clear();
        } else {
            current.push(character);
        }
    }
    parameters.push(current.trim().to_owned());
    parameters
}

fn unquote_header_value(value: &str) -> String {
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value);
    let mut output = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            output.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            output.push(character);
        }
    }
    if escaped {
        output.push('\\');
    }
    output
}

fn percent_decode_utf8(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            let text = std::str::from_utf8(hex).ok()?;
            decoded.push(u8::from_str_radix(text, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

pub(super) fn file_name_from_response_url(url: &reqwest::Url) -> Option<String> {
    let segment = url.path_segments()?.rfind(|segment| !segment.is_empty())?;
    percent_decode_utf8(segment).filter(|value| !value.is_empty())
}

pub(super) fn ensure_same_source(
    downloaded: u64,
    previous: &SourceValidator,
    current: &SourceValidator,
) -> Result<(), EngineError> {
    if downloaded > 0
        && !matches!(previous, SourceValidator::None)
        && !matches!(current, SourceValidator::None)
        && previous != current
    {
        return Err(EngineError::SourceChanged);
    }
    Ok(())
}

pub(super) fn response_size(headers: &HeaderMap, resumed: u64) -> TransferSize {
    if resumed > 0 {
        if let Some(total_bytes) = content_range_total(headers) {
            return TransferSize::Known { total_bytes };
        }
    }
    headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(TransferSize::Unknown, |remaining| TransferSize::Known {
            total_bytes: resumed + remaining,
        })
}

pub(super) fn ensure_same_size(
    downloaded: u64,
    previous: &TransferSize,
    current: &TransferSize,
) -> Result<(), EngineError> {
    if downloaded > 0 {
        match (previous, current) {
            (
                TransferSize::Known {
                    total_bytes: previous,
                },
                TransferSize::Known {
                    total_bytes: current,
                },
            ) if previous == current => {}
            (TransferSize::Known { .. }, _) => return Err(EngineError::SourceChanged),
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn validate_content_range(
    headers: &HeaderMap,
    expected_start: u64,
    expected_end: Option<u64>,
    expected_total: Option<u64>,
) -> Result<(), EngineError> {
    let value = headers
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .ok_or(EngineError::InvalidContentRange)?;
    let bounds = value
        .strip_prefix("bytes ")
        .and_then(|value| value.split('/').next())
        .and_then(|value| value.split_once('-'))
        .and_then(|(start, end)| Some((start.parse::<u64>().ok()?, end.parse::<u64>().ok()?)))
        .ok_or(EngineError::InvalidContentRange)?;
    if bounds.1 < bounds.0
        || bounds.0 != expected_start
        || expected_end.is_some_and(|end| end != bounds.1)
    {
        return Err(EngineError::InvalidContentRange);
    }
    if expected_total.is_some_and(|total| content_range_total(headers) != Some(total)) {
        return Err(EngineError::InvalidContentRange);
    }
    if headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length != bounds.1 - bounds.0 + 1)
    {
        return Err(EngineError::InvalidContentRange);
    }
    Ok(())
}

pub(super) fn content_range_total(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split('/').nth(1))
        .and_then(|value| value.parse::<u64>().ok())
}

pub(super) async fn detect_file_name(
    source: &DownloadSource,
    proxy: &ResolvedProxy,
) -> Option<String> {
    let client = build_client(proxy, source).ok()?;
    let request = apply_source_headers(client.head(&source.url), source).ok()?;
    let response = time::timeout(METADATA_TIMEOUT, request.send())
        .await
        .ok()?
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    content_disposition_file_name(response.headers())
        .or_else(|| file_name_from_response_url(response.url()))
}
