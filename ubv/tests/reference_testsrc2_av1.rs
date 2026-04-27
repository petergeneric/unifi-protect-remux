//! Reference-fixture test: parses `testdata/essence/testsrc2_av1.ubv`,
//! serialises the result with the same call `ubv-info --json` makes, and
//! asserts the output matches the checked-in `testdata/essence/testsrc2_av1.json`
//! byte-for-byte.
//!
//! Sibling of `reference_testsrc2.rs` for the AV1 (track 1004) path. Catches
//! parser/serde regressions specific to AV1 record handling.

use std::path::PathBuf;

use ubv::reader::{open_ubv, parse_ubv};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn ubv_info_json_matches_reference_av1() {
    let ubv_path = workspace_root().join("testdata/essence/testsrc2_av1.ubv");
    let json_path = workspace_root().join("testdata/essence/testsrc2_av1.json");

    assert!(
        ubv_path.exists(),
        "reference fixture missing: {}",
        ubv_path.display()
    );
    assert!(
        json_path.exists(),
        "reference JSON missing: {}",
        json_path.display()
    );

    let mut reader = open_ubv(&ubv_path).expect("open testsrc2_av1.ubv");
    let parsed = parse_ubv(&mut reader).expect("parse testsrc2_av1.ubv");

    let actual = serde_json::to_string(&parsed).expect("serialise to JSON");
    let expected = std::fs::read_to_string(&json_path).expect("read reference JSON");
    let expected = expected.trim_end_matches('\n');

    if actual != expected {
        let mismatch = actual
            .as_bytes()
            .iter()
            .zip(expected.as_bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(actual.len().min(expected.len()));
        let start = mismatch.saturating_sub(60);
        let end_a = (mismatch + 60).min(actual.len());
        let end_e = (mismatch + 60).min(expected.len());
        panic!(
            "ubv-info JSON does not match {}\n  \
             actual len:   {}\n  \
             expected len: {}\n  \
             first diff at byte: {}\n  \
             actual   …{}…\n  \
             expected …{}…\n\n\
             To regenerate (after a deliberate format change):\n  \
             cargo run -p ubv-info -- --json testdata/essence/testsrc2_av1.ubv > testdata/essence/testsrc2_av1.json",
            json_path.display(),
            actual.len(),
            expected.len(),
            mismatch,
            &actual[start..end_a],
            &expected[start..end_e],
        );
    }
}
