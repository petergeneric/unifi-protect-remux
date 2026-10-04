#[allow(dead_code)]
#[path = "../../testdata/support/records.rs"]
mod fixtures;

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
