use std::{io, path::Path, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Semaphore,
    time::timeout,
};

use crate::downloads::{
    error::AppError,
    manager::DownloadManager,
    model::{BrowserDownloadInput, DownloadItem},
};

pub const BROWSER_BRIDGE_PORT: u16 = 17_846;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_CONNECTIONS: usize = 32;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const TOKEN_FILE: &str = "browser-bridge.token";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserIntegration {
    pub available: bool,
    pub port: u16,
    pub token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BridgeRequest {
    input: BrowserDownloadInput,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BridgeResponse<T> {
    ok: bool,
    data: Option<T>,
    error: Option<String>,
}

pub async fn start(app: AppHandle, manager: Arc<DownloadManager>) -> BrowserIntegration {
    let token = match load_or_create_token(&app).await {
        Ok(token) => token,
        Err(error) => {
            eprintln!("Fluxor browser bridge token unavailable: {error}");
            return integration(false, String::new());
        }
    };
    let address = format!("127.0.0.1:{BROWSER_BRIDGE_PORT}");
    let listener = match TcpListener::bind(&address).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("Fluxor browser bridge unavailable on {address}: {error}");
            return integration(false, String::new());
        }
    };
    let integration = integration(true, token.clone());
    let semaphore = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    tauri::async_runtime::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let Ok(permit) = semaphore.clone().try_acquire_owned() else {
                continue;
            };
            let manager = Arc::clone(&manager);
            let token = token.clone();
            tauri::async_runtime::spawn(async move {
                let _permit = permit;
                let _ = timeout(REQUEST_TIMEOUT, handle_connection(stream, manager, &token)).await;
            });
        }
    });
    integration
}

fn integration(available: bool, token: String) -> BrowserIntegration {
    BrowserIntegration {
        available,
        port: BROWSER_BRIDGE_PORT,
        token,
    }
}

async fn load_or_create_token(app: &AppHandle) -> io::Result<String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| io::Error::other(error.to_string()))?;
    tokio::fs::create_dir_all(&directory).await?;
    let path = directory.join(TOKEN_FILE);
    if let Ok(token) = tokio::fs::read_to_string(&path).await {
        let token = token.trim().to_owned();
        if is_valid_token(&token) {
            return Ok(token);
        }
    }
    let token = uuid::Uuid::new_v4().simple().to_string();
    tokio::fs::write(&path, &token).await?;
    set_private_permissions(&path).await?;
    Ok(token)
}

fn is_valid_token(token: &str) -> bool {
    token.len() == 32 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

async fn set_private_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
    }
    Ok(())
}

async fn handle_connection(
    mut stream: TcpStream,
    manager: Arc<DownloadManager>,
    token: &str,
) -> io::Result<()> {
    let mut request = read_request_head(&mut stream).await?;
    let origin_allowed = request.origin.as_deref().map_or(true, is_extension_origin);
    let token_allowed = request.token.as_deref() == Some(token);
    if !origin_allowed || (request.method != "OPTIONS" && !token_allowed) {
        let response = json_error(
            403,
            "Token de Fluxor inválido o extensión no autorizada",
            request.origin.as_deref(),
        );
        stream.write_all(response.as_bytes()).await?;
        return stream.shutdown().await;
    }
    request.body = read_body(&mut stream, request.body_prefix, request.content_length).await?;
    let origin = request.origin.as_deref();
    let response = match (request.method.as_str(), request.path.as_str()) {
        ("OPTIONS", _) => http_response(204, "text/plain", "", origin),
        ("GET", "/v1/status") => {
            let body = serde_json::to_string(&BridgeResponse {
                ok: true,
                data: Some(integration(true, String::new())),
                error: None,
            })
            .unwrap_or_default();
            http_response(200, "application/json", &body, origin)
        }
        ("POST", "/v1/downloads") => match serde_json::from_slice::<BridgeRequest>(&request.body) {
            Ok(request) => bridge_download(manager, request.input, origin).await,
            Err(_) => json_error(400, "La solicitud no tiene un formato válido", origin),
        },
        _ => json_error(404, "Ruta no encontrada", origin),
    };
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
}

async fn bridge_download(
    manager: Arc<DownloadManager>,
    input: BrowserDownloadInput,
    origin: Option<&str>,
) -> String {
    match manager.add_from_browser(input).await {
        Ok(item) => json_success(item, origin),
        Err(error) => json_app_error(error, origin),
    }
}

fn json_success(item: DownloadItem, origin: Option<&str>) -> String {
    let body = serde_json::to_string(&BridgeResponse {
        ok: true,
        data: Some(item),
        error: None,
    })
    .unwrap_or_default();
    http_response(201, "application/json", &body, origin)
}

fn json_app_error(error: AppError, origin: Option<&str>) -> String {
    json_error(422, &error.to_string(), origin)
}

fn json_error(status: u16, message: &str, origin: Option<&str>) -> String {
    let body = serde_json::to_string(&BridgeResponse::<()> {
        ok: false,
        data: None,
        error: Some(message.to_owned()),
    })
    .unwrap_or_default();
    http_response(status, "application/json", &body, origin)
}

fn http_response(status: u16, content_type: &str, body: &str, origin: Option<&str>) -> String {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        422 => "Unprocessable Content",
        _ => "Error",
    };
    let cors_headers = if let Some(origin) = origin {
        format!(
            "Access-Control-Allow-Origin: {origin}\r\n\
             Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
             Access-Control-Allow-Headers: Content-Type, X-Fluxor-Token\r\n\
             Vary: Origin\r\n"
        )
    } else {
        String::new()
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\n\
         {cors_headers}\
         Connection: close\r\n\r\n{body}",
        body.len()
    )
}

struct HttpRequest {
    method: String,
    path: String,
    origin: Option<String>,
    token: Option<String>,
    content_length: usize,
    body_prefix: Vec<u8>,
    body: Vec<u8>,
}

async fn read_request_head(stream: &mut TcpStream) -> io::Result<HttpRequest> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    let header_end = loop {
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "request headers ended early",
            ));
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request too large",
            ));
        }
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid request headers"))?;
    let mut lines = headers.split("\r\n");
    let mut request_line = lines.next().unwrap_or_default().split_whitespace();
    let method = request_line.next().unwrap_or_default().to_owned();
    let path = request_line
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_owned();
    let mut content_length = 0_usize;
    let mut origin = None;
    let mut token = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value.trim().parse().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid content length")
            })?;
        } else if name.eq_ignore_ascii_case("origin") {
            origin = Some(value.trim().to_owned());
        } else if name.eq_ignore_ascii_case("x-fluxor-token") {
            token = Some(value.trim().to_owned());
        }
    }
    if content_length > MAX_REQUEST_BYTES || header_end + content_length > MAX_REQUEST_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "request too large",
        ));
    }
    Ok(HttpRequest {
        method,
        path,
        origin,
        token,
        content_length,
        body_prefix: bytes[header_end..].to_vec(),
        body: Vec::new(),
    })
}

async fn read_body(
    stream: &mut TcpStream,
    mut bytes: Vec<u8>,
    content_length: usize,
) -> io::Result<Vec<u8>> {
    bytes.truncate(content_length);
    let mut buffer = [0_u8; 8192];
    while bytes.len() < content_length {
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "request body ended early",
            ));
        }
        let remaining = content_length - bytes.len();
        bytes.extend_from_slice(&buffer[..read.min(remaining)]);
    }
    Ok(bytes)
}

fn is_extension_origin(origin: &str) -> bool {
    origin.starts_with("chrome-extension://") || origin.starts_with("moz-extension://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_only_enable_cors_for_the_expected_extension() {
        let response = http_response(
            200,
            "application/json",
            "{}",
            Some("chrome-extension://example"),
        );
        assert!(response.contains("chrome-extension://example"));
        assert!(response.contains("Content-Length: 2"));

        let denied = json_error(403, "denied", None);
        assert!(!denied.contains("Access-Control-Allow-Origin"));
        assert!(is_extension_origin("moz-extension://example"));
        assert!(!is_extension_origin("https://example.com"));
        assert!(is_valid_token("0123456789abcdef0123456789abcdef"));
        assert!(!is_valid_token("short"));
    }
}
