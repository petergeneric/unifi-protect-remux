#[allow(dead_code)]
#[path = "../../testdata/support/records.rs"]
mod fixtures;

#[test]
fn conversion_warns_and_extracts_media_before_stale_trailing_skip() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.ubv");
    let destination = dir.path().join("outputs");
    std::fs::create_dir(&destination).unwrap();
    let mut bytes = Vec::new();
    fixtures::partition(&mut bytes);
    fixtures::append_record(&mut bytes, 7, false, &[0, 0, 0, 2, 0x65, 0x88]);
    fixtures::append_record(&mut bytes, 6, true, &[0; 4096]);
    let end = bytes.len();
    bytes[end - 4..].copy_from_slice(&2048u32.to_be_bytes());
    std::fs::write(&input, bytes).unwrap();
    let config = remux_lib::RemuxConfig {
        mp4: false,
        with_audio: false,
        output_folder: destination.to_str().unwrap().into(),
        ..Default::default()
    };
    let mut warnings = Vec::new();
    let result = remux_lib::process_file(input.to_str().unwrap(), &config, &mut |event| {
        if let remux_lib::ProgressEvent::Log(remux_lib::LogLevel::Warn, message) = event {
            warnings.push(message);
        }
    })
    .unwrap();
    assert!(warnings.iter().any(|w| w.contains("trailing Skip")));
    assert!(result.errors.is_empty());
    assert_eq!(result.output_files.len(), 1);
    assert_eq!(
        std::fs::read(&result.output_files[0]).unwrap(),
        [0, 0, 0, 1, 0x65, 0x88]
    );
}

#[test]
fn conversion_rejects_incomplete_input_before_creating_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.ubv");
    let destination = dir.path().join("outputs");
    for (bytes, message) in [
        (fixtures::incomplete_video().0, "back-size mismatch"),
        (fixtures::zero_clock_rate().0, "invalid clock rate"),
    ] {
        std::fs::write(&input, bytes).unwrap();
        for mp4 in [false, true] {
            let config = remux_lib::RemuxConfig {
                mp4,
                output_folder: destination.to_str().unwrap().into(),
                ..Default::default()
            };
            let error = remux_lib::process_file(input.to_str().unwrap(), &config, &mut |_| {})
                .err()
                .expect("incomplete input must fail");
            assert!(error.to_string().contains(message));
            assert!(!destination.exists());
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }
}
