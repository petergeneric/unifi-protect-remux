use std::path::Path;

use sha2::{Digest, Sha256};
use ubv::reader::open_ubv;

/// Parse a .ubv.gz file, serialise to JSON, and verify the SHA-256 checksum matches.
fn check_json_checksum(ubv_gz_path: &str, expected_sha256: &str, timed_sha256: &str) {
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
    assert!(ubv.read_status.is_complete());
    let json = serde_json::to_string(&ubv).expect("failed to serialise UbvFile to JSON");

    let hash = Sha256::digest(json.as_bytes());
    let actual_sha256 = format!("{:x}", hash);

    assert_eq!(
        actual_sha256, expected_sha256,
        "JSON SHA-256 mismatch for {}",
        ubv_gz_path
    );
    // These fixture outputs now include read status and the correctly parsed
    // F1 00 Skip. These timed-only goldens were computed separately
    // from the pre-change parser, omitting its invalid Skip interpretation.
    #[derive(serde::Serialize)]
    struct TimedFile {
        partitions: Vec<ubv::partition::Partition>,
    }
    let mut partitions = ubv.partitions;
    for partition in &mut partitions {
        partition
            .entries
            .retain(|e| !matches!(e, ubv::partition::PartitionEntry::Untimed(_)));
    }
    let timed = serde_json::to_vec(&TimedFile { partitions }).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&timed)),
        timed_sha256,
        "timed fixture content changed"
    );
}

#[test]
fn test_json_checksum_old_h264() {
    check_json_checksum(
        "../testdata/anonymised/sample1_0_rotating_1770769558568.ubv.gz",
        "40c577629a70b66dbd91d38efd8409d1369a856bbedb82a861e15fb6c6425d96",
        "f6379c9724d7708eaf0e16167c5fc628e5ec9e0b0df19d2f6c07777bd912d2e5",
    );
}

#[test]
fn test_json_checksum_new_h264() {
    check_json_checksum(
        "../testdata/anonymised/sample2_0_rotating_1683867159535.ubv.gz",
        "7617081bcd67458fa89bc20bca918e5508d092063483df98f3fba5a12288aa3f",
        "d96ba2f52c4641227e46f92a0c1a0a6bd018d7c06bfe99dfea93551da0313d99",
    );
}

#[test]
fn test_json_checksum_hevc() {
    check_json_checksum(
        "../testdata/anonymised/sample3_0_rotating_1770695988380.ubv.gz",
        "cac99e0df01c2e02c5ee7605e880438584ff615b36902b5c8f9dbb762c81f243",
        "6a2cdcad461dce6d63c62714f5fd717f5840b18bee5d5388abb0a3fca51bc11a",
    );
}
