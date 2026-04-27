//! Reference-fixture tests against `testdata/essence/testsrc2_av1.ubv`.
//!
//! Sibling of `reference_testsrc2.rs` for the AV1 (track 1004) path:
//!
//! - `demux_testsrc2_av1_matches_reference_md5` runs the raw demux path
//!   (mp4=false) and asserts the md5 of the OBU bitstream. AV1 demux is a
//!   verbatim byte-copy out of the .ubv (no Annex-B reframing), so the hash
//!   is platform-independent and stable across FFmpeg versions.
//! - `mp4_mux_testsrc2_av1_produces_expected_stream` runs the default
//!   MP4-mux path and opens the result with ffmpeg-next to verify the AV1
//!   sample entry, resolution, and frame count.
//!
//! The fixture is video-only by design: A/V interleaving is already covered
//! by the H.264+AAC `testsrc2.ubv` reference; this fixture's job is to
//! exercise the AV1-specific OBU plumbing.

extern crate ffmpeg_next as ffmpeg;

use std::path::{Path, PathBuf};

use ffmpeg::codec::Id as CodecId;
use ffmpeg::media::Type;
use remux_lib::{ProgressEvent, RemuxConfig, process_file};

/// md5 of the OBU elementary stream demuxed from testsrc2_av1.ubv.
const EXPECTED_AV1_MD5: &str = "914d55fa47fde882cc1338aae9a241f0";

fn fixture_ubv() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/essence/testsrc2_av1.ubv")
}

#[test]
fn demux_testsrc2_av1_matches_reference_md5() {
    let ubv = fixture_ubv();
    assert!(ubv.exists(), "reference fixture missing: {}", ubv.display());

    let tmpdir = make_tmpdir();
    let config = RemuxConfig {
        mp4: false,
        with_audio: true,
        with_video: true,
        output_folder: tmpdir.to_string_lossy().into_owned(),
        ..RemuxConfig::default()
    };

    let mut outputs: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let result = process_file(ubv.to_str().unwrap(), &config, &mut |ev| match ev {
        ProgressEvent::OutputGenerated { path } => outputs.push(path),
        ProgressEvent::PartitionError { error, .. } => errors.push(error),
        _ => {}
    })
    .expect("process_file returned an error");

    assert!(errors.is_empty(), "partition errors: {errors:?}");
    assert!(
        result.errors.is_empty(),
        "FileResult carried errors: {:?}",
        result.errors
    );

    let av1_path = outputs
        .iter()
        .find(|p| p.ends_with(".av1"))
        .unwrap_or_else(|| panic!("no .av1 output produced; outputs={outputs:?}"));

    let av1_md5 = file_md5(av1_path);
    assert_eq!(
        av1_md5, EXPECTED_AV1_MD5,
        "AV1 OBU bitstream md5 mismatch (file: {av1_path})"
    );
}

fn file_md5(path: &str) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    format!("{:x}", md5::compute(&bytes))
}

// Dimensions / frame count of the testsrc2_av1 source (see
// scripts/create-test-ubv-round-trip.sh --codec av1): 320x240 @ 30 fps for 2 s.
const EXPECTED_WIDTH: u32 = 320;
const EXPECTED_HEIGHT: u32 = 240;
const EXPECTED_VIDEO_FRAMES: usize = 60;

#[test]
fn mp4_mux_testsrc2_av1_produces_expected_stream() {
    let ubv = fixture_ubv();
    assert!(ubv.exists(), "reference fixture missing: {}", ubv.display());

    let tmpdir = make_tmpdir();
    let config = RemuxConfig {
        output_folder: tmpdir.to_string_lossy().into_owned(),
        ..RemuxConfig::default()
    };

    let mut outputs: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let result = process_file(ubv.to_str().unwrap(), &config, &mut |ev| match ev {
        ProgressEvent::OutputGenerated { path } => outputs.push(path),
        ProgressEvent::PartitionError { error, .. } => errors.push(error),
        _ => {}
    })
    .expect("process_file returned an error");

    assert!(errors.is_empty(), "partition errors: {errors:?}");
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

    verify_mp4_output(Path::new(&outputs[0]));
}

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
        CodecId::AV1,
        "expected AV1 output, got {:?}",
        params.id()
    );

    let (w, h) = unsafe {
        let raw = params.as_ptr();
        ((*raw).width as u32, (*raw).height as u32)
    };
    assert_eq!(w, EXPECTED_WIDTH, "width mismatch");
    assert_eq!(h, EXPECTED_HEIGHT, "height mismatch");

    let packet_count = ictx
        .packets()
        .filter(|(s, _)| s.index() == stream_index)
        .count();
    assert_eq!(
        packet_count, EXPECTED_VIDEO_FRAMES,
        "video frame count mismatch: expected {EXPECTED_VIDEO_FRAMES}, got {packet_count}"
    );
}

fn make_tmpdir() -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "remux-ref-testsrc2-av1-{}-{}",
        std::process::id(),
        nanos
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
