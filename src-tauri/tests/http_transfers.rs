//! Real HTTP transfers against an adversarial local origin; no GUI or external network.
use chrono::Utc;
use downloader_lib::downloads::{
    engine::{
        DownloadEngine, EngineError, EngineInput, EngineOutput, EngineProgress, ResolvedProxy,
    },
    model::*,
};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicU64, Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::watch,
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
struct Request {
    method: String,
    range: Option<String>,
}
struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    truncate: bool,
}
struct Origin {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
    task: JoinHandle<()>,
}
impl Drop for Origin {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Origin {
    async fn new(handler: impl Fn(&Request) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let handler = Arc::new(handler);
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let handler = handler.clone();
                let recorded = recorded.clone();
                tokio::spawn(async move {
                    let mut raw = Vec::new();
                    let mut byte = [0];
                    while !raw.ends_with(b"\r\n\r\n") && raw.len() < 16384 {
                        if socket.read(&mut byte).await.unwrap_or(0) == 0 {
                            return;
                        }
                        raw.push(byte[0]);
                    }
                    let raw = String::from_utf8(raw).unwrap();
                    let request = Request {
                        method: raw.split_whitespace().next().unwrap().to_owned(),
                        range: raw.lines().find_map(|line| {
                            line.split_once(':')
                                .filter(|(name, _)| name.eq_ignore_ascii_case("range"))
                                .map(|(_, v)| v.trim().to_owned())
                        }),
                    };
                    recorded.lock().unwrap().push(request.clone());
                    let reply = handler(&request);
                    let mut headers =
                        format!("HTTP/1.1 {} Test\r\nConnection: close\r\n", reply.status);
                    for (key, value) in reply.headers {
                        headers += &format!("{key}: {value}\r\n");
                    }
                    headers += "\r\n";
                    if socket.write_all(headers.as_bytes()).await.is_err() {
                        return;
                    }
                    if request.method != "HEAD" {
                        let len = if reply.truncate {
                            reply.body.len() / 2
                        } else {
                            reply.body.len()
                        };
                        let _ = socket.write_all(&reply.body[..len]).await;
                    }
                });
            }
        });
        Self {
            url,
            requests,
            task,
        }
    }
}
struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("fluxor-http-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn input(origin: &Origin, dir: &Sandbox, threads: u8, force: bool) -> EngineInput {
    EngineInput {
        item: DownloadItem {
            id: "test".into(),
            file_name: "payload.bin".into(),
            category: DownloadCategory::Other,
            state: DownloadState::Queued,
            source: DownloadSource {
                url: origin.url.clone(),
                headers: vec![],
                cookies: vec![],
                proxy: ProxySelection::Direct,
                force_single_stream: force,
            },
            destination: "".into(),
            transfer: TransferProgress {
                downloaded_bytes: 0,
                size: TransferSize::Unknown,
                validator: SourceValidator::None,
                resume: ResumeSupport::Unknown,
            },
            telemetry: TransferTelemetry::default(),
            threads,
            speed_limit_bytes: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        destination_dir: dir.0.clone(),
        proxy: ResolvedProxy::Direct,
        speed_limit: Arc::new(AtomicU64::new(0)),
    }
}
async fn run(input: EngineInput) -> Result<EngineOutput, EngineError> {
    let (tx, _rx) = watch::channel::<Option<EngineProgress>>(None);
    tokio::time::timeout(
        Duration::from_secs(12),
        DownloadEngine::run(input, CancellationToken::new(), tx),
    )
    .await
    .expect("engine hung")
}
fn payload() -> Vec<u8> {
    (0..5 * 1024 * 1024).map(|i| (i % 251) as u8).collect()
}
fn reply(req: &Request, data: &[u8], ranges: bool) -> Reply {
    let mut headers = vec![("ETag".into(), "\"v1\"".into())];
    if ranges {
        headers.push(("Accept-Ranges".into(), "bytes".into()));
    }
    let (status, body) = if let Some(range) = req
        .range
        .as_deref()
        .filter(|_| req.method != "HEAD" && ranges)
    {
        let (a, b) = range
            .strip_prefix("bytes=")
            .unwrap()
            .split_once('-')
            .unwrap();
        let a: usize = a.parse().unwrap();
        let b: usize = if b.is_empty() {
            data.len() - 1
        } else {
            b.parse().unwrap()
        };
        headers.push((
            "Content-Range".into(),
            format!("bytes {a}-{b}/{}", data.len()),
        ));
        (206, data[a..=b].to_vec())
    } else {
        (200, data.to_vec())
    };
    headers.push(("Content-Length".into(), body.len().to_string()));
    Reply {
        status,
        headers,
        body,
        truncate: false,
    }
}
#[tokio::test]
async fn forced_single_makes_exactly_one_get_and_validates_full_file() {
    let data = payload();
    let expected = data.clone();
    let origin = Origin::new(move |r| reply(r, &data, true)).await;
    let dir = Sandbox::new();
    let output = run(input(&origin, &dir, 8, true)).await.unwrap();
    assert!(matches!(output.telemetry.mode, TransferMode::Single { .. }));
    assert_eq!(std::fs::read(dir.0.join("payload.bin")).unwrap(), expected);
    let requests = origin.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].range.as_deref(), Some("bytes=0-"));
}
#[tokio::test]
async fn segmented_transfer_merges_exact_bytes() {
    let data = payload();
    let expected = data.clone();
    let origin = Origin::new(move |r| reply(r, &data, true)).await;
    let dir = Sandbox::new();
    let output = run(input(&origin, &dir, 4, false)).await.unwrap();
    assert!(matches!(output.telemetry.mode, TransferMode::Segmented));
    assert!(matches!(output.resume, ResumeSupport::Supported));
    assert_eq!(std::fs::read(dir.0.join("payload.bin")).unwrap(), expected);
    assert!(origin.requests.lock().unwrap().len() >= 4);
}
#[tokio::test]
async fn head_rejected_and_no_accept_ranges_header_can_still_segment() {
    let data = payload();
    let expected = data.clone();
    let origin = Origin::new(move |r| {
        let mut out = reply(r, &data, true);
        out.headers.retain(|(k, _)| k != "Accept-Ranges");
        if r.method == "HEAD" {
            out.status = 405;
        }
        out
    })
    .await;
    let dir = Sandbox::new();
    let output = run(input(&origin, &dir, 4, false)).await.unwrap();
    assert!(matches!(output.telemetry.mode, TransferMode::Segmented));
    assert_eq!(std::fs::read(dir.0.join("payload.bin")).unwrap(), expected);
}
#[tokio::test]
async fn lying_range_advertisement_falls_back_safely() {
    let data = payload();
    let expected = data.clone();
    let origin = Origin::new(move |r| {
        let mut out = reply(r, &data, false);
        out.headers.push(("Accept-Ranges".into(), "bytes".into()));
        out
    })
    .await;
    let dir = Sandbox::new();
    let output = run(input(&origin, &dir, 4, false)).await.unwrap();
    assert!(matches!(output.telemetry.mode, TransferMode::Single { .. }));
    assert!(matches!(output.resume, ResumeSupport::Unsupported { .. }));
    assert_eq!(std::fs::read(dir.0.join("payload.bin")).unwrap(), expected);
}
#[tokio::test]
async fn partial_response_must_cover_requested_tail_before_finalizing() {
    let origin = Origin::new(|_| Reply {
        status: 206,
        headers: vec![
            ("Content-Length".into(), "10".into()),
            ("Content-Range".into(), "bytes 0-9/100".into()),
        ],
        body: vec![1; 10],
        truncate: false,
    })
    .await;
    let dir = Sandbox::new();
    assert!(matches!(
        run(input(&origin, &dir, 1, true)).await,
        Err(EngineError::InvalidContentRange)
    ));
    assert!(!dir.0.join("payload.bin").exists());
}
#[tokio::test]
async fn invalid_range_total_cannot_finalize() {
    let origin = Origin::new(|_| Reply {
        status: 206,
        headers: vec![
            ("Content-Length".into(), "10".into()),
            ("Content-Range".into(), "bytes 0-9/9".into()),
        ],
        body: vec![1; 10],
        truncate: false,
    })
    .await;
    let dir = Sandbox::new();
    assert!(matches!(
        run(input(&origin, &dir, 1, true)).await,
        Err(EngineError::InvalidContentRange)
    ));
    assert!(!dir.0.join("payload.bin").exists());
}
#[tokio::test]
async fn disconnected_single_stream_resumes_without_duplicating_bytes() {
    let data = payload();
    let expected = data.clone();
    let first = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let origin = Origin::new(move |r| {
        let mut out = reply(r, &data, true);
        if r.method == "GET" && first.swap(false, std::sync::atomic::Ordering::SeqCst) {
            out.truncate = true;
        }
        out
    })
    .await;
    let dir = Sandbox::new();
    let mut job = input(&origin, &dir, 1, true);
    assert!(matches!(run(job.clone()).await, Err(EngineError::Request)));
    let partial = std::fs::metadata(dir.0.join(".payload.bin.fluxor.part"))
        .unwrap()
        .len();
    assert!(partial > 0);
    job.item.transfer.downloaded_bytes = partial;
    job.item.transfer.size = TransferSize::Known {
        total_bytes: expected.len() as u64,
    };
    job.item.transfer.validator = SourceValidator::Etag {
        value: "\"v1\"".into(),
    };
    run(job).await.unwrap();
    assert_eq!(std::fs::read(dir.0.join("payload.bin")).unwrap(), expected);
    assert_eq!(
        origin.requests.lock().unwrap().last().unwrap().range,
        Some(format!("bytes={partial}-"))
    );
}
#[tokio::test]
async fn ignored_resume_and_changed_identity_preserve_partial() {
    for changed in [false, true] {
        let data = payload();
        let origin = Origin::new(move |r| {
            let mut out = reply(r, &data, changed);
            if changed {
                for (k, v) in &mut out.headers {
                    if k == "ETag" {
                        *v = "\"v2\"".into();
                    }
                }
            }
            out
        })
        .await;
        let dir = Sandbox::new();
        let bytes = vec![7; 100];
        let partial = dir.0.join(".payload.bin.fluxor.part");
        std::fs::write(&partial, &bytes).unwrap();
        let mut job = input(&origin, &dir, 1, false);
        job.item.transfer.validator = SourceValidator::Etag {
            value: "\"v1\"".into(),
        };
        let error = run(job).await.unwrap_err();
        assert!(matches!(
            error,
            EngineError::ResumeRejected | EngineError::SourceChanged
        ));
        assert_eq!(std::fs::read(partial).unwrap(), bytes);
        assert!(!dir.0.join("payload.bin").exists());
    }
}
#[tokio::test]
async fn authentication_failure_is_not_a_range_failure() {
    for status in [401, 403, 404, 429, 503] {
        let origin = Origin::new(move |_| Reply {
            status,
            headers: vec![("Content-Length".into(), "0".into())],
            body: vec![],
            truncate: false,
        })
        .await;
        let dir = Sandbox::new();
        std::fs::write(dir.0.join(".payload.bin.fluxor.part"), [1, 2, 3]).unwrap();
        let error = run(input(&origin, &dir, 1, false)).await.unwrap_err();
        assert!(matches!(error,EngineError::HttpStatus(s) if s==status));
        assert_eq!(error.recoverable(), matches!(status, 429 | 503));
    }
}
#[tokio::test]
async fn cancelled_rate_limited_transfer_exits_promptly() {
    let data = payload();
    let origin = Origin::new(move |r| reply(r, &data, true)).await;
    let dir = Sandbox::new();
    let mut job = input(&origin, &dir, 1, true);
    job.speed_limit = Arc::new(AtomicU64::new(1));
    let token = CancellationToken::new();
    let cancel = token.clone();
    let (tx, _) = watch::channel(None);
    let task = tokio::spawn(DownloadEngine::run(job, token, tx));
    tokio::time::sleep(Duration::from_millis(100)).await;
    cancel.cancel();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap(),
        Err(EngineError::Cancelled)
    ));
    assert!(!dir.0.join("payload.bin").exists());
}

#[tokio::test]
async fn raising_or_removing_live_limit_unblocks_current_chunk() {
    for new_limit in [0, 100_000_000] {
        let data = vec![42; 128 * 1024];
        let expected = data.clone();
        let origin = Origin::new(move |r| reply(r, &data, true)).await;
        let dir = Sandbox::new();
        let mut job = input(&origin, &dir, 1, true);
        let limit = Arc::new(AtomicU64::new(1));
        job.speed_limit = limit.clone();
        let task = tokio::spawn(run(job));
        tokio::time::sleep(Duration::from_millis(100)).await;
        limit.store(new_limit, std::sync::atomic::Ordering::Relaxed);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("old low limit kept transfer asleep")
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read(dir.0.join("payload.bin")).unwrap(), expected);
    }
}
#[tokio::test]
async fn configured_limit_applies_to_the_whole_transfer() {
    let data = vec![42; 128 * 1024];
    let origin = Origin::new(move |r| reply(r, &data, true)).await;
    let dir = Sandbox::new();
    let mut job = input(&origin, &dir, 1, true);
    job.speed_limit = Arc::new(AtomicU64::new(128 * 1024));
    let started = std::time::Instant::now();
    run(job).await.unwrap();
    assert!(started.elapsed() >= Duration::from_millis(900));
}

#[tokio::test]
async fn unknown_length_finishes_only_on_clean_eof() {
    let data = vec![17; 100_000];
    let expected = data.clone();
    let origin = Origin::new(move |r| {
        let mut out = reply(r, &data, false);
        out.headers.retain(|(name, _)| name != "Content-Length");
        out
    })
    .await;
    let dir = Sandbox::new();
    let output = run(input(&origin, &dir, 1, true)).await.unwrap();
    assert!(matches!(output.size, TransferSize::Unknown));
    assert_eq!(output.downloaded_bytes, expected.len() as u64);
    assert_eq!(std::fs::read(dir.0.join("payload.bin")).unwrap(), expected);
}
#[tokio::test]
async fn existing_destination_is_never_overwritten() {
    let data = vec![42; 100];
    let origin = Origin::new(move |r| reply(r, &data, true)).await;
    let dir = Sandbox::new();
    let path = dir.0.join("payload.bin");
    std::fs::write(&path, b"existing user file").unwrap();
    assert!(matches!(
        run(input(&origin, &dir, 1, true)).await,
        Err(EngineError::DestinationExists)
    ));
    assert_eq!(std::fs::read(path).unwrap(), b"existing user file");
}
#[tokio::test]
async fn interrupted_segments_are_retained_and_resumed() {
    let data = payload();
    let expected = data.clone();
    let interrupted = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let origin = Origin::new(move |r| {
        let mut out = reply(r, &data, true);
        if r.method == "GET"
            && out.body.len() > 1
            && !interrupted.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            out.truncate = true;
        }
        out
    })
    .await;
    let dir = Sandbox::new();
    let job = input(&origin, &dir, 4, false);
    assert!(matches!(
        run(job.clone()).await,
        Err(EngineError::SegmentInterrupted)
    ));
    assert!(!dir.0.join("payload.bin").exists());
    assert!(std::fs::read_dir(&dir.0)
        .unwrap()
        .filter_map(Result::ok)
        .any(
            |p| p.file_name().to_string_lossy().contains(".fluxor.part.")
                && p.metadata().unwrap().len() > 0
        ));
    run(job).await.unwrap();
    assert_eq!(std::fs::read(dir.0.join("payload.bin")).unwrap(), expected);
}
#[tokio::test]
async fn single_stream_keeps_publishing_while_limited() {
    let data = vec![42; 10_000];
    let origin = Origin::new(move |r| reply(r, &data, true)).await;
    let dir = Sandbox::new();
    let mut job = input(&origin, &dir, 1, true);
    job.speed_limit = Arc::new(AtomicU64::new(1000));
    let token = CancellationToken::new();
    let (tx, mut rx) = watch::channel(None);
    let task = tokio::spawn(DownloadEngine::run(job, token.clone(), tx));
    let mut samples = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), async {
        while samples.len() < 3 {
            rx.changed().await.unwrap();
            let sample = rx.borrow().clone().unwrap();
            if matches!(sample.telemetry.phase, TransferPhase::Transferring) {
                samples.push(sample);
            }
        }
    })
    .await
    .expect("missing telemetry under low bandwidth");
    assert!(samples.last().unwrap().downloaded_bytes > 0);
    token.cancel();
    assert!(matches!(task.await.unwrap(), Err(EngineError::Cancelled)));
}

#[tokio::test]
async fn truncated_chunked_body_never_finalizes_unknown_size() {
    let origin = Origin::new(|_| Reply {
        status: 200,
        headers: vec![("Transfer-Encoding".into(), "chunked".into())],
        body: b"10\r\n0123456789abcdef\r\n0\r\n\r\n".to_vec(),
        truncate: true,
    })
    .await;
    let dir = Sandbox::new();
    assert!(matches!(
        run(input(&origin, &dir, 1, true)).await,
        Err(EngineError::Request)
    ));
    assert!(!dir.0.join("payload.bin").exists());
}
#[tokio::test]
async fn pause_has_stable_partial_size_then_resumes_exactly() {
    let data = payload();
    let expected = data.clone();
    let origin = Origin::new(move |r| reply(r, &data, true)).await;
    let dir = Sandbox::new();
    let mut job = input(&origin, &dir, 1, true);
    job.speed_limit = Arc::new(AtomicU64::new(128 * 1024));
    let token = CancellationToken::new();
    let (tx, mut rx) = watch::channel(None);
    let task = tokio::spawn(DownloadEngine::run(job.clone(), token.clone(), tx));
    let sample = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            rx.changed().await.unwrap();
            let sample = rx.borrow().clone().unwrap();
            if sample.downloaded_bytes > 0 {
                break sample;
            }
        }
    })
    .await
    .unwrap();
    token.cancel();
    assert!(matches!(task.await.unwrap(), Err(EngineError::Cancelled)));
    let path = dir.0.join(".payload.bin.fluxor.part");
    let length = std::fs::metadata(&path).unwrap().len();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(std::fs::metadata(&path).unwrap().len(), length);
    job.item.transfer = TransferProgress {
        downloaded_bytes: length,
        size: sample.size,
        validator: sample.validator,
        resume: sample.resume,
    };
    job.speed_limit
        .store(0, std::sync::atomic::Ordering::Relaxed);
    run(job).await.unwrap();
    assert_eq!(std::fs::read(dir.0.join("payload.bin")).unwrap(), expected);
}
