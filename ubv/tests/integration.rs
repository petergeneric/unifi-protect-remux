use std::path::Path;

use sha2::{Digest, Sha256};
use ubv::reader::open_ubv;

/// Parse a .ubv.gz file, serialise to JSON, and verify the SHA-256 checksum matches.
///
/// The checksums changed when untimed records (format `F1 00`, no DTS) started to
/// be parsed: each sample ends with one such Skip record, formerly misread as a
/// `Skip` entry with `dts` = its size and a bogus `data_size`, now an `Untimed`
/// entry. `test_old_files_unchanged_apart_from_untimed_records` checks that
/// everything else is unchanged.
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
        "../testdata/anonymised/sample1_0_rotating_1770769558568.ubv.gz",
        "8f252c3ca348042238fb5cc564f81aa162885993b31740cec1e49509d757427d",
    );
}

#[test]
fn test_json_checksum_new_h264() {
    check_json_checksum(
        "../testdata/anonymised/sample2_0_rotating_1683867159535.ubv.gz",
        "c624970e2a6b5839c394bf43194eb77d794e7fd1d5b069c667d838c9fc6d81e3",
    );
}

#[test]
fn test_json_checksum_hevc() {
    check_json_checksum(
        "../testdata/anonymised/sample3_0_rotating_1770695988380.ubv.gz",
        "fc435bcb2cea02f485446ccc83a65f46f1cda6c9d721622a75dc5d6a5745e79c",
    );
}

/// Hash of the parsed JSON with every record that lacks a DTS removed: the new
/// `Untimed` entries and, as the parser before the fix produced them, the
/// `Skip` / `SmartEvent` entries with `clock_rate == 0` that came from misreading
/// those same records (format `F1 00`). The constants below were computed with
/// the parser *before* the fix, so this test proves that, apart from those
/// records, these files parse exactly as before.
fn json_sha256_without_untimed(ubv_gz_path: &str) -> Option<String> {
    let ubv_file = Path::new(ubv_gz_path);
    if !ubv_file.exists() {
        eprintln!("Skipping integration test: file not found at {ubv_gz_path}");
        return None;
    }
    let mut reader = open_ubv(ubv_file).expect("failed to open UBV file");
    let ubv = ubv::reader::parse_ubv(&mut reader).expect("failed to parse UBV file");
    let mut value = serde_json::to_value(&ubv).expect("failed to serialise UbvFile");
    for p in value["partitions"].as_array_mut().unwrap() {
        p["entries"].as_array_mut().unwrap().retain(|e| {
            if e.get("Untimed").is_some() {
                return false;
            }
            for k in ["Skip", "SmartEvent"] {
                if let Some(m) = e.get(k) {
                    return m["clock_rate"].as_u64() != Some(0);
                }
            }
            true
        });
    }
    let json = serde_json::to_string(&value).unwrap();
    Some(format!("{:x}", Sha256::digest(json.as_bytes())))
}

#[test]
fn test_old_files_unchanged_apart_from_untimed_records() {
    let cases = [
        (
            "../testdata/anonymised/sample1_0_rotating_1770769558568.ubv.gz",
            "20fe6a815c2beca96c774be6f8e012208b1c44ba70ac900c1273d04c7f2dff71",
        ),
        (
            "../testdata/anonymised/sample2_0_rotating_1683867159535.ubv.gz",
            "7b2fac3773042797328b15ac0f2731fb7b1bec4a71170f68932e9bb9d7db6707",
        ),
        (
            "../testdata/anonymised/sample3_0_rotating_1770695988380.ubv.gz",
            "99082552d365b3127b228771d58dd60d21873ab94be6fa9eb1f5897404dd1b31",
        ),
    ];
    let mut mismatches = Vec::new();
    for (path, expected) in cases {
        if let Some(actual) = json_sha256_without_untimed(path)
            && actual != expected
        {
            mismatches.push(format!("{path}: expected {expected}, got {actual}"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "filtered JSON SHA-256 mismatch:\n{}",
        mismatches.join("\n")
    );
}

/// The sample files end with a Skip record (track 6, format `F1 00`, no DTS)
/// that runs exactly to the end of the (preallocated, 1 GiB) file. Before the
/// fix its size was read from the wrong bytes.
#[test]
fn sample_trailing_skip_is_read_as_untimed_record() {
    use ubv::partition::PartitionEntry;

    let cases = [
        (
            "../testdata/anonymised/sample1_0_rotating_1770769558568.ubv.gz",
            584_356u32,
        ),
        (
            "../testdata/anonymised/sample2_0_rotating_1683867159535.ubv.gz",
            347_520,
        ),
        (
            "../testdata/anonymised/sample3_0_rotating_1770695988380.ubv.gz",
            281_900,
        ),
    ];
    for (path, size) in cases {
        let ubv_file = Path::new(path);
        if !ubv_file.exists() {
            eprintln!("Skipping integration test: file not found at {path}");
            continue;
        }
        let mut reader = open_ubv(ubv_file).expect("failed to open UBV file");
        let ubv = ubv::reader::parse_ubv(&mut reader).expect("failed to parse UBV file");

        let last = ubv.partitions.last().unwrap().entries.last().unwrap();
        let PartitionEntry::Untimed(u) = last else {
            panic!("{path}: last entry is not an untimed record: {last:?}");
        };
        assert_eq!(
            (u.track_id, u.format_code.0, u.data_size),
            (6, 0xF100, size),
            "{path}"
        );
        // 12-byte prefix + data + padding + 4-byte back-size == end of file.
        let pad = (4 - (u.data_offset + size as u64) % 4) % 4;
        assert_eq!(u.data_offset + size as u64 + pad + 4, 1 << 30, "{path}");
    }
}
