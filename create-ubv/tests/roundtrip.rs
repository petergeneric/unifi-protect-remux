//! Integration test: synthesise a .ubv from a tiny MP4 fixture and verify
//! that ubv's parser can read it back with the expected structure.

use std::path::PathBuf;

use ubv::partition::PartitionEntry;
use ubv::reader::{open_ubv, parse_ubv};
use create_ubv::{synth_from_mp4, SynthConfig};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny.mp4")
}

#[test]
fn synth_roundtrips_through_ubv_parser() {
    let mp4 = fixture_path();
    if !mp4.exists() {
        panic!(
            "test fixture {} is missing. Regenerate with:\n  \
             ffmpeg -y -f lavfi -i testsrc=size=64x64:rate=30:duration=0.2 \\\n    \
             -c:v libx264 -profile:v baseline -level 3.0 \\\n    \
             -g 2 -x264-params keyint=2:scenecut=0 \\\n    \
             -pix_fmt yuv420p -movflags +faststart \\\n    \
             {}",
            mp4.display(),
            mp4.display(),
        );
    }

    let tmpdir = tempdir();
    let ubv_path = tmpdir.join("synthetic.ubv");

    synth_from_mp4(&mp4, &ubv_path, &SynthConfig::default()).expect("synth_from_mp4 failed");

    let mut reader = open_ubv(&ubv_path).expect("open_ubv failed");
    let parsed = parse_ubv(&mut reader).expect("parse_ubv failed");

    assert_eq!(parsed.partitions.len(), 1, "expected exactly one partition");
    let part = &parsed.partitions[0];

    // Count frames and verify structure.
    let frames: Vec<_> = part
        .entries
        .iter()
        .filter_map(|e| match e {
            PartitionEntry::Frame(f) => Some(f),
            _ => None,
        })
        .collect();
    assert!(!frames.is_empty(), "expected at least one video frame");
    assert!(frames[0].header.keyframe, "first frame must be a keyframe");
    assert!(
        frames.iter().all(|f| ubv::track::is_video_track(f.header.track_id)),
        "synth output should only contain video frames"
    );

    // Clock sync must have been emitted — partition should contain one.
    let clock_syncs = part
        .entries
        .iter()
        .filter(|e| matches!(e, PartitionEntry::ClockSync(_)))
        .count();
    assert_eq!(clock_syncs, 1, "expected exactly one clock sync record");

    // DTS must be non-decreasing.
    let mut last_dts = 0u64;
    for (i, f) in frames.iter().enumerate() {
        assert!(
            f.header.dts >= last_dts,
            "DTS regressed at frame {}: {} < {}",
            i, f.header.dts, last_dts
        );
        last_dts = f.header.dts;
    }
}

/// Minimal tempdir helper: picks a unique directory under std::env::temp_dir()
/// and creates it. No cleanup — test runs are short-lived and the OS reaps /tmp.
fn tempdir() -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("create-ubv-test-{}-{}", std::process::id(), nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
