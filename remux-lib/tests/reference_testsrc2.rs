//! Reference-fixture test: demuxes `testdata/essence/testsrc2.ubv` to raw
//! H.264 and AAC bitstreams and verifies their md5 hashes against pinned values.
//!
//! Catches regressions in the demux path (UBV record envelope parsing, NAL
//! length-prefix → Annex B start code conversion, ADTS audio passthrough).
//! Output bytes are pure rearrangement of the input UBV — no FFmpeg is
//! involved on the write path — so the hashes are platform-independent and
//! stable across FFmpeg versions.
//!
//! `testdata/essence/testsrc2.ubv` is a synthetic fixture produced by
//! `create-ubv` from an `ffmpeg testsrc2` source (see `create-ubv/README.md`).

use std::path::PathBuf;

use remux_lib::{process_file, ProgressEvent, RemuxConfig};

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
    assert!(
        ubv.exists(),
        "reference fixture missing: {}",
        ubv.display()
    );

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
