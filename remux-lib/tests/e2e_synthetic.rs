//! End-to-end smoke test that validates the FFmpeg pipeline by:
//!   1. Synthesising a `.ubv` file from a checked-in MP4 fixture via `create-ubv`.
//!   2. Running it back through `remux_lib::process_file` (which drives FFmpeg
//!      probing, packet wrapping, and the MOV muxer).
//!   3. Re-opening the resulting MP4 with ffmpeg-next to verify codec,
//!      resolution, and frame count.
//!
//! This exists to catch FFmpeg ABI/behaviour drift that compile-time checks
//! would miss — function signatures may match while runtime behaviour changes
//! between FFmpeg minor versions.

extern crate ffmpeg_next as ffmpeg;

use std::path::{Path, PathBuf};

use ffmpeg::codec::Id as CodecId;
use ffmpeg::media::Type;
use remux_lib::{process_file, ProgressEvent, RemuxConfig};
use create_ubv::{synth_from_mp4, SynthConfig};

const EXPECTED_WIDTH: u32 = 64;
const EXPECTED_HEIGHT: u32 = 64;
const EXPECTED_FRAMES: usize = 6;

fn fixture_mp4() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny.mp4")
}

#[test]
fn remux_pipeline_round_trips_synthetic_ubv() {
    let mp4 = fixture_mp4();
    assert!(
        mp4.exists(),
        "fixture missing: {}. See create-ubv/tests/roundtrip.rs for the regen command.",
        mp4.display()
    );

    let tmpdir = tempdir();

    // The UBV filename drives the output basename. Use `_0_rotating_X.ubv`
    // format so process_file doesn't emit a "rotating/timelapse not supported"
    // warning (it only matches `_2_rotating_` and `_timelapse_`).
    let ubv_path = tmpdir.join("synth_0_rotating_0.ubv");
    synth_from_mp4(&mp4, &ubv_path, &SynthConfig::default())
        .expect("create-ubv failed to build synthetic .ubv from fixture");

    // Run the full remux pipeline and collect OutputGenerated events so we
    // can locate the produced MP4. All other events are ignored.
    let config = RemuxConfig {
        output_folder: tmpdir.to_string_lossy().into_owned(),
        ..RemuxConfig::default()
    };
    let mut outputs: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut progress = |ev: ProgressEvent| match ev {
        ProgressEvent::OutputGenerated { path } => outputs.push(path),
        ProgressEvent::PartitionError { error, .. } => errors.push(error),
        _ => {}
    };

    let result = process_file(
        ubv_path.to_str().unwrap(),
        &config,
        &mut progress,
    )
    .expect("process_file returned an error");

    assert!(
        errors.is_empty(),
        "partition errors during remux: {errors:?}"
    );
    assert!(
        result.errors.is_empty(),
        "FileResult carried errors: {:?}",
        result.errors
    );
    assert_eq!(
        outputs.len(),
        1,
        "expected exactly one MP4 output, got: {outputs:?}"
    );

    let mp4_out = Path::new(&outputs[0]);
    assert!(mp4_out.exists(), "output MP4 missing: {}", mp4_out.display());
    let size = std::fs::metadata(mp4_out).unwrap().len();
    assert!(size > 1024, "output MP4 suspiciously small: {size} bytes");

    // Validate the muxed MP4 with FFmpeg — this is the teeth of the test.
    verify_mp4_output(mp4_out);
}

/// Open the produced MP4 with ffmpeg-next and verify codec/resolution/frames.
fn verify_mp4_output(path: &Path) {
    ffmpeg::init().expect("ffmpeg init failed");
    let mut ictx = ffmpeg::format::input(&path)
        .unwrap_or_else(|e| panic!("open output {}: {e}", path.display()));

    let stream = ictx
        .streams()
        .best(Type::Video)
        .expect("output MP4 has no video stream");
    let stream_index = stream.index();
    let params = stream.parameters();
    assert_eq!(
        params.id(),
        CodecId::H264,
        "expected H.264 output, got {:?}",
        params.id()
    );

    // Read raw width/height out of codecpar.
    let (w, h) = unsafe {
        let raw = params.as_ptr();
        ((*raw).width as u32, (*raw).height as u32)
    };
    assert_eq!(w, EXPECTED_WIDTH, "width mismatch");
    assert_eq!(h, EXPECTED_HEIGHT, "height mismatch");

    // Count packets from the video stream as a proxy for frame count.
    // (packet-count == frame-count for non-fragmented H.264 in MP4.)
    let packet_count = ictx
        .packets()
        .filter(|(s, _)| s.index() == stream_index)
        .count();
    assert_eq!(
        packet_count, EXPECTED_FRAMES,
        "frame count mismatch: expected {EXPECTED_FRAMES}, got {packet_count}"
    );
}

fn tempdir() -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("remux-e2e-{}-{}", std::process::id(), nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
