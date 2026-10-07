use std::process::Command;
#[allow(dead_code)]
#[path = "../../testdata/support/records.rs"]
mod fixtures;

#[test]
fn recovered_skip_warns_and_json_command_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("copied-during-recording.ubv");
    let (bytes, start) = fixtures::stale_trailing_skip();
    std::fs::write(&path, bytes).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ubv-info"))
        .arg("--json")
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["read_status"]["end_reason"], "recovered_skip");
    assert_eq!(json["read_status"]["validated_end_offset"], start);
    assert_eq!(json["read_status"]["warnings"].as_array().unwrap().len(), 1);
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("warning:")
    );
}

// The checked-in schema can retain concise descriptions independently of Rust docs.
fn remove_descriptions(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            object.remove("description");
            object.values_mut().for_each(remove_descriptions);
        }
        serde_json::Value::Array(array) => array.iter_mut().for_each(remove_descriptions),
        _ => {}
    }
}

#[test]
fn partial_json_is_visible_but_command_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("incomplete.ubv");
    let (bytes, bad) = fixtures::incomplete_video();
    std::fs::write(&path, bytes).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ubv-info"))
        .arg("--json")
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["read_status"]["end_reason"], "error");
    assert_eq!(json["read_status"]["validated_end_offset"], bad);
    assert_eq!(
        json["partitions"][0]["entries"].as_array().unwrap().len(),
        1
    );
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("incomplete inspection")
    );
}

#[test]
fn complete_input_and_generated_schema_include_status_and_untimed_records() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("complete.ubv");
    let mut bytes = Vec::new();
    fixtures::partition(&mut bytes);
    fixtures::append_record(&mut bytes, 10, true, &[0; 28]);
    std::fs::write(&path, &bytes).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ubv-info"))
        .arg("--json")
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["read_status"]["end_reason"], "eof");
    assert!(
        json["partitions"][0]["entries"][0]["Untimed"]
            .get("dts")
            .is_none()
    );
    let schema = Command::new(env!("CARGO_BIN_EXE_ubv-info"))
        .arg("--schema")
        .output()
        .unwrap();
    assert!(schema.status.success());
    let mut schema: serde_json::Value = serde_json::from_slice(&schema.stdout).unwrap();
    let mut checked_schema: serde_json::Value =
        serde_json::from_str(include_str!("../info.schema.json")).unwrap();
    remove_descriptions(&mut schema);
    remove_descriptions(&mut checked_schema);
    assert_eq!(schema, checked_schema);
    assert!(schema["properties"].get("read_status").is_some());
    assert!(schema["definitions"].get("UntimedRecord").is_some());
}

#[test]
fn zero_clock_rate_reports_partial_json_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("zero-rate.ubv");
    let (bytes, bad) = fixtures::zero_clock_rate();
    std::fs::write(&path, bytes).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ubv-info"))
        .arg("--json")
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["read_status"]["end_reason"], "error");
    assert_eq!(json["read_status"]["validated_end_offset"], bad);
    assert_eq!(
        json["partitions"][0]["entries"].as_array().unwrap().len(),
        1
    );
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("invalid clock rate")
    );
}
