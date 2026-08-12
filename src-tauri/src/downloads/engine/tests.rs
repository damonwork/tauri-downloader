use std::time::{Duration, Instant};

use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_RANGE, ETAG, LAST_MODIFIED,
};

use super::http::{
    confirmed_segment_validator, content_disposition_file_name, content_range_total,
    ensure_same_source, file_name_from_response_url, response_validator, resume_support,
    validate_content_range, validate_segment_response_validator,
};
use super::rate::{transfer_duration, TransferRateEstimator};
use super::segments::{
    segments_downloaded, split_ranges, SegmentMetadata, SegmentRuntime, SEGMENT_COMPLETED,
    SEGMENT_DOWNLOADING,
};
use super::storage::{
    merge_segments, partial_path, persist_segment_metadata, read_segment_metadata,
    require_segment_metadata, segment_path,
};
use super::{
    ensure_segment_partials_compatible, segmented_failure_allows_single_stream,
    single_stream_reason, supports_segmented_transfer, EngineError, ProbeResult, ResumeSupport,
    SourceValidator, TransferSize, MIN_SEGMENT_SIZE,
};
use crate::downloads::model::SegmentState;

#[test]
fn split_ranges_covers_the_file_without_gaps() {
    let total = MIN_SEGMENT_SIZE * 5 + 17;
    let ranges = split_ranges(total, 4);

    assert_eq!(ranges.first().map(|range| range.0), Some(0));
    assert_eq!(ranges.last().map(|range| range.1), Some(total - 1));
    for pair in ranges.windows(2) {
        assert_eq!(pair[0].1 + 1, pair[1].0);
    }
    assert!(ranges.len() <= 4);
}

#[test]
fn segment_runtime_reports_only_its_own_range_and_progress() {
    let runtime = SegmentRuntime::new(2, 200, 299, 20);
    runtime.set_state(SEGMENT_DOWNLOADING);
    runtime.mark_activity(30);

    let segment = runtime.snapshot(4_096);

    assert_eq!(segment.index, 2);
    assert_eq!((segment.start_byte, segment.end_byte), (200, Some(299)));
    assert_eq!(segment.downloaded_bytes, 50);
    assert_eq!(segment.speed_bytes, 4_096);
    assert!(matches!(segment.state, SegmentState::Downloading));
    assert!(segment.last_activity_at.is_some());
    assert_eq!(segments_downloaded(std::slice::from_ref(&segment)), 50);
}

#[test]
fn completed_segments_do_not_report_stale_speed() {
    let runtime = SegmentRuntime::new(0, 0, 99, 50);
    runtime.set_state(SEGMENT_COMPLETED);

    assert_eq!(runtime.snapshot(4_096).speed_bytes, 0);
}

#[test]
fn transfer_rate_smooths_variation_and_brief_gaps() {
    let started_at = Instant::now();
    let one_mib = 1024 * 1024;
    let mut rate = TransferRateEstimator::new(0, started_at);

    let stable = rate.sample(4 * one_mib, started_at + Duration::from_secs(1));
    let increased = rate.sample(12 * one_mib, started_at + Duration::from_secs(2));
    let brief_gap = rate.sample(12 * one_mib, started_at + Duration::from_millis(2_300));

    assert_eq!(stable, 4 * one_mib);
    assert!((4 * one_mib..8 * one_mib).contains(&increased));
    assert_eq!(brief_gap, increased);
}

#[test]
fn transfer_rate_decays_and_reaches_zero_after_a_sustained_stall() {
    let started_at = Instant::now();
    let mut rate = TransferRateEstimator::new(0, started_at);
    let active = rate.sample(4_000_000, started_at + Duration::from_secs(1));
    let decayed = rate.sample(4_000_000, started_at + Duration::from_millis(2_500));
    let stopped = rate.sample(4_000_000, started_at + Duration::from_millis(5_100));

    assert!(decayed > 0);
    assert!(decayed < active);
    assert_eq!(stopped, 0);
}

#[test]
fn transfer_rate_uses_the_full_elapsed_time_after_a_delayed_tick() {
    let started_at = Instant::now();
    let mut rate = TransferRateEstimator::new(0, started_at);

    assert_eq!(
        rate.sample(3_000_000, started_at + Duration::from_secs(3)),
        1_000_000
    );
}

#[test]
fn single_stream_reason_explains_why_segments_are_not_used() {
    assert!(single_stream_reason(None, 8).contains("confirmar"));
    let small = ProbeResult {
        size: TransferSize::Known {
            total_bytes: MIN_SEGMENT_SIZE - 1,
        },
        validator: SourceValidator::Etag {
            value: "\"safe\"".to_owned(),
        },
        accepts_ranges: true,
    };
    assert!(single_stream_reason(Some(&small), 8).contains("pequeño"));
}

#[test]
fn range_server_without_validator_can_use_segments() {
    let mediafire_like = ProbeResult {
        size: TransferSize::Known {
            total_bytes: 262_430_628,
        },
        validator: SourceValidator::None,
        accepts_ranges: true,
    };

    assert!(supports_segmented_transfer(&mediafire_like, 8));
    assert!(!supports_segmented_transfer(&mediafire_like, 1));
}

#[test]
fn validatorless_segments_require_the_same_total_size_to_resume() {
    let previous = TransferSize::Known { total_bytes: 100 };
    let same = TransferSize::Known { total_bytes: 100 };
    let changed = TransferSize::Known { total_bytes: 120 };

    assert!(ensure_segment_partials_compatible(
        &previous,
        &same,
        &SourceValidator::None,
        &SourceValidator::None,
    )
    .is_ok());
    assert!(matches!(
        ensure_segment_partials_compatible(
            &previous,
            &changed,
            &SourceValidator::None,
            &SourceValidator::None,
        ),
        Err(EngineError::SourceChanged)
    ));
    assert!(matches!(
        ensure_segment_partials_compatible(
            &previous,
            &same,
            &SourceValidator::None,
            &SourceValidator::Etag {
                value: "\"new\"".to_owned(),
            },
        ),
        Err(EngineError::SourceChanged)
    ));
}

#[test]
fn range_confirmation_establishes_and_workers_enforce_a_validator() {
    let first = SourceValidator::Etag {
        value: "\"v1\"".to_owned(),
    };
    let changed = SourceValidator::Etag {
        value: "\"v2\"".to_owned(),
    };

    let established = confirmed_segment_validator(&SourceValidator::None, first.clone())
        .expect("range confirmation should establish the validator");
    assert_eq!(established, first);
    assert!(matches!(
        confirmed_segment_validator(&established, changed),
        Err(EngineError::SourceChanged)
    ));
    assert!(validate_segment_response_validator(&SourceValidator::None, &established).is_err());
}

#[tokio::test]
async fn segment_metadata_is_synced_before_partials_are_reusable() {
    let directory = std::env::temp_dir().join(format!("fluxor-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&directory).await.unwrap();
    let path = directory.join("segments.json");
    let metadata = SegmentMetadata {
        total_bytes: 262_430_628,
        validator: SourceValidator::Etag {
            value: "\"v1\"".to_owned(),
        },
        threads: 8,
        ranges: split_ranges(262_430_628, 8),
    };

    persist_segment_metadata(&path, &metadata).await.unwrap();
    let restored = read_segment_metadata(&path).await.unwrap().unwrap();

    assert_eq!(restored.total_bytes, metadata.total_bytes);
    assert_eq!(restored.validator, metadata.validator);
    assert_eq!(restored.threads, metadata.threads);
    assert_eq!(restored.ranges, metadata.ranges);
    tokio::fs::remove_dir_all(directory).await.unwrap();
}

#[test]
fn segment_partials_without_metadata_are_rejected() {
    assert!(matches!(
        require_segment_metadata(true, None),
        Err(EngineError::SourceChanged)
    ));
    assert!(require_segment_metadata(false, None).is_ok());
}

#[tokio::test]
async fn cancelled_merge_keeps_segments_available_for_resume() {
    let directory = std::env::temp_dir().join(format!("fluxor-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&directory).await.unwrap();
    let file_name = "archive.zip";
    let segment = segment_path(&directory, file_name, 0);
    tokio::fs::write(&segment, b"partial").await.unwrap();
    let destination = partial_path(&directory, file_name);
    let cancellation = tokio_util::sync::CancellationToken::new();
    cancellation.cancel();

    let result = merge_segments(&destination, &directory, file_name, 1, &cancellation).await;

    assert!(matches!(result, Err(EngineError::Cancelled)));
    assert!(tokio::fs::try_exists(segment).await.unwrap());
    assert!(!tokio::fs::try_exists(destination).await.unwrap());
    tokio::fs::remove_dir_all(directory).await.unwrap();
}

#[test]
fn content_range_must_match_both_requested_bounds() {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_RANGE,
        HeaderValue::from_static("bytes 100-199/1000"),
    );

    assert!(validate_content_range(&headers, 100, Some(199), Some(1000)).is_ok());
    assert!(matches!(
        validate_content_range(&headers, 100, Some(299), Some(1000)),
        Err(EngineError::InvalidContentRange)
    ));
    assert!(matches!(
        validate_content_range(&headers, 100, Some(199), Some(2000)),
        Err(EngineError::InvalidContentRange)
    ));
    assert_eq!(content_range_total(&headers), Some(1000));
}

#[test]
fn malformed_content_range_is_rejected() {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_RANGE, HeaderValue::from_static("items 0-10/20"));

    assert!(matches!(
        validate_content_range(&headers, 0, None, None),
        Err(EngineError::InvalidContentRange)
    ));
}

#[test]
fn partial_without_a_durable_validator_can_resume_by_exact_range() {
    assert!(ensure_same_source(100, &SourceValidator::None, &SourceValidator::None).is_ok());
}

#[test]
fn conflicting_durable_validators_are_rejected() {
    let previous = SourceValidator::Etag {
        value: "\"old\"".to_owned(),
    };
    let current = SourceValidator::Etag {
        value: "\"new\"".to_owned(),
    };

    assert!(matches!(
        ensure_same_source(100, &previous, &current),
        Err(EngineError::SourceChanged)
    ));
}

#[test]
fn content_disposition_prefers_utf8_file_name() {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static(
            "attachment; filename=report.pdf; filename*=UTF-8''informe%20final.pdf",
        ),
    );

    assert_eq!(
        content_disposition_file_name(&headers).as_deref(),
        Some("informe final.pdf")
    );
}

#[test]
fn content_disposition_handles_quoted_semicolons() {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"report; final.pdf\""),
    );

    assert_eq!(
        content_disposition_file_name(&headers).as_deref(),
        Some("report; final.pdf")
    );
}

#[test]
fn final_response_url_is_percent_decoded() {
    let url = reqwest::Url::parse("https://cdn.example.com/files/video%20final.mp4").unwrap();

    assert_eq!(
        file_name_from_response_url(&url).as_deref(),
        Some("video final.mp4")
    );
}

#[test]
fn remote_segment_failures_can_degrade_to_one_stream() {
    assert!(segmented_failure_allows_single_stream(
        &EngineError::ResumeRejected
    ));
    assert!(segmented_failure_allows_single_stream(
        &EngineError::Request
    ));
    assert!(segmented_failure_allows_single_stream(
        &EngineError::SourceChanged
    ));
    assert!(!segmented_failure_allows_single_stream(
        &EngineError::DestinationExists
    ));
    assert!(!segmented_failure_allows_single_stream(
        &EngineError::SegmentTask
    ));
}

#[test]
fn rejected_resume_requires_restart() {
    assert!(!EngineError::ResumeRejected.recoverable());
    assert!(EngineError::Request.recoverable());
}

#[test]
fn resume_support_follows_range_capability() {
    assert!(matches!(resume_support(true), ResumeSupport::Supported));
    assert!(matches!(
        resume_support(false),
        ResumeSupport::Unsupported { .. }
    ));
}

#[test]
fn weak_etag_uses_last_modified_instead() {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    headers.insert(ETAG, HeaderValue::from_static("W/\"weak\""));
    headers.insert(
        LAST_MODIFIED,
        HeaderValue::from_static("Tue, 11 Aug 2026 00:00:00 GMT"),
    );

    assert!(matches!(
        response_validator(&headers),
        SourceValidator::LastModified { .. }
    ));
}

#[test]
fn bandwidth_duration_uses_the_aggregate_byte_rate() {
    assert_eq!(
        transfer_duration(512 * 1024, 1024 * 1024),
        Duration::from_millis(500)
    );
}
