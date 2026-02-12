use std::path::Path;

use sha2::{Digest, Sha256};
use ubv::reader::open_ubv;

/// Parse a .ubv.gz file, serialise to JSON, and verify the SHA-256 checksum matches.
fn check_json_checksum(ubv_gz_path: &str, expected_sha256: &str) {
    let ubv_file = Path::new(ubv_gz_path);

    if !ubv_file.exists() {
        eprintln!(
            "Skipping integration test: file not found at {}",
            ubv_gz_path
        );
        return;
    }

    let mut reader = open_ubv(ubv_file).expect("failed to open UBV file");
    let ubv = ubv::reader::parse_ubv(&mut reader).expect("failed to parse UBV file");
    let json = serde_json::to_string(&ubv).expect("failed to serialise UbvFile to JSON");

    let hash = Sha256::digest(json.as_bytes());
    let actual_sha256 = format!("{:x}", hash);

    assert_eq!(
        actual_sha256, expected_sha256,
        "JSON SHA-256 mismatch for {}",
        ubv_gz_path
    );
}

#[test]
fn test_json_checksum_old_h264() {
    check_json_checksum(
        "../testdata/sample1_0_rotating_1683867159535.ubv.gz",
        "3188bdd2d308ba85f1575c38d93d0b68979f3a5614f47de7cc96b806fe7540cb",
    );
}

#[test]
fn test_json_checksum_new_h264() {
    check_json_checksum(
        "../testdata/sample2_0_rotating_1770769558568.ubv.gz",
        "580a50c408851ffc8bc950ac370af44fdd97215f70c680c100aa87a5278c6934",
    );
}

#[test]
fn test_json_checksum_hevc() {
    check_json_checksum(
        "../testdata/sample3_0_rotating_1770695988380.ubv.gz",
        "19917af8d619ff068e3945240bb4a41de656cca06865a640e988787b5bc77e85",
    );
}
