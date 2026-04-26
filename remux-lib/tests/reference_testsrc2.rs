//! Reference-fixture tests against `testdata/essence/testsrc2.ubv`.
//!
//! - `demux_testsrc2_matches_reference_md5` runs the raw demux path (mp4=false)
//!   and asserts md5 hashes of the H.264 and AAC bitstreams. Output bytes are a
//!   pure rearrangement of the input UBV — no FFmpeg on the write path — so the
//!   hashes are platform-independent and stable across FFmpeg versions.
//! - `mp4_mux_testsrc2_produces_expected_stream` runs the default MP4-mux path
//!   (mp4=true) and opens the result with ffmpeg-next to verify codec,
//!   resolution, and frame count. This is the only place FFmpeg ABI/behaviour
//!   drift on the mux path would be caught.
//!
//! `testdata/essence/testsrc2.ubv` is a synthetic fixture produced by
//! `create-ubv` from an `ffmpeg testsrc2` source (see `create-ubv/README.md`).

extern crate ffmpeg_next as ffmpeg;

use std::path::{Path, PathBuf};

use ffmpeg::codec::Id as CodecId;
use ffmpeg::media::Type;
use remux_lib::{ProgressEvent, RemuxConfig, process_file};

/// md5 of the Annex-B H.264 elementary stream demuxed from testsrc2.ubv.
const EXPECTED_H264_MD5: &str = "304044c46466cc7926a17ace81be9114";

/// md5 of the raw ADTS AAC elementary stream demuxed from testsrc2.ubv.
const EXPECTED_AAC_MD5: &str = "e2baa6d0f55ba3fd85f86fc3e8c591d6";

fn fixture_ubv() -> PathBuf {
    // remux-lib/ → workspace root → testdata/essence/
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/essence/testsrc2.ubv")
}

#[test]
fn demux_testsrc2_matches_reference_md5() {
    let ubv = fixture_ubv();
    assert!(ubv.exists(), "reference fixture missing: {}", ubv.display());

    let tmpdir = make_tmpdir();
    let config = RemuxConfig {
        // mp4=false selects the raw demux path: writes .h264 + .aac elementary
        // streams alongside each other instead of muxing into MP4.
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

    let h264_path = outputs
        .iter()
        .find(|p| p.ends_with(".h264"))
        .unwrap_or_else(|| panic!("no .h264 output produced; outputs={outputs:?}"));
    let aac_path = outputs
        .iter()
        .find(|p| p.ends_with(".aac"))
        .unwrap_or_else(|| panic!("no .aac output produced; outputs={outputs:?}"));

    let h264_md5 = file_md5(h264_path);
    let aac_md5 = file_md5(aac_path);

    assert_eq!(
        h264_md5, EXPECTED_H264_MD5,
        "H.264 bitstream md5 mismatch (file: {h264_path})"
    );
    assert_eq!(
        aac_md5, EXPECTED_AAC_MD5,
        "AAC bitstream md5 mismatch (file: {aac_path})"
    );
}

fn file_md5(path: &str) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    format!("{:x}", md5::compute(&bytes))
}

// Dimensions / frame count of the testsrc2 source (see
// scripts/create-test-ubv-round-trip.sh): 640x480 @ 30 fps for 5 s.
const EXPECTED_WIDTH: u32 = 640;
const EXPECTED_HEIGHT: u32 = 480;
const EXPECTED_VIDEO_FRAMES: usize = 150;

#[test]
fn mp4_mux_testsrc2_produces_expected_stream() {
    let ubv = fixture_ubv();
    assert!(ubv.exists(), "reference fixture missing: {}", ubv.display());

    let tmpdir = make_tmpdir();
    // mp4=true (the default) drives the FFmpeg MOV muxer — the path that would
    // break on FFmpeg ABI drift between minor versions.
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

    let (w, h) = unsafe {
        let raw = params.as_ptr();
        ((*raw).width as u32, (*raw).height as u32)
    };
    assert_eq!(w, EXPECTED_WIDTH, "width mismatch");
    assert_eq!(h, EXPECTED_HEIGHT, "height mismatch");

    // Packet count == frame count for non-fragmented H.264 in MP4.
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
        "remux-ref-testsrc2-{}-{}",
        std::process::id(),
        nanos
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
