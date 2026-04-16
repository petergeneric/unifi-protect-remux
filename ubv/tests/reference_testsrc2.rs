//! Reference-fixture test: parses `testdata/essence/testsrc2.ubv`, serialises
//! the result with the same call `ubv-info --json` makes, and asserts the
//! output matches the checked-in `testdata/essence/testsrc2.json` byte-for-byte.
//!
//! Catches regressions in the parser, serde derives, or any field rename that
//! would change the JSON shape consumed by downstream tooling.

use std::path::PathBuf;

use ubv::reader::{open_ubv, parse_ubv};

fn workspace_root() -> PathBuf {
    // ubv/ → workspace root
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn ubv_info_json_matches_reference() {
    let ubv_path = workspace_root().join("testdata/essence/testsrc2.ubv");
    let json_path = workspace_root().join("testdata/essence/testsrc2.json");

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

    let mut reader = open_ubv(&ubv_path).expect("open testsrc2.ubv");
    let parsed = parse_ubv(&mut reader).expect("parse testsrc2.ubv");

    // Match `ubv-info --json` exactly: compact (non-pretty) serde_json output.
    let actual = serde_json::to_string(&parsed).expect("serialise to JSON");
    let expected = std::fs::read_to_string(&json_path).expect("read reference JSON");
    // Allow a trailing newline in the checked-in file (some editors auto-add one).
    let expected = expected.trim_end_matches('\n');

    if actual != expected {
        // Emit useful diagnostics: lengths, first-byte mismatch, and a short
        // window around the divergence.
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
             cargo run -p ubv-info -- --json testdata/essence/testsrc2.ubv > testdata/essence/testsrc2.json",
            json_path.display(),
            actual.len(),
            expected.len(),
            mismatch,
            &actual[start..end_a],
            &expected[start..end_e],
        );
    }
}
