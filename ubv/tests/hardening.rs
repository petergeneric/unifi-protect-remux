use std::io::{Cursor, Seek, SeekFrom};
use ubv::partition::PartitionEntry;
use ubv::reader::{EndReason, parse_ubv, parse_ubv_partial};
use ubv::record::read_record;

#[allow(dead_code)]
#[path = "../../testdata/support/records.rs"]
mod fixtures;
use fixtures::*;

#[test]
fn indexes_and_large_skips_do_not_hide_later_partitions_or_frames() {
    let mut bytes = Vec::new();
    append_record(&mut bytes, 6, true, &[0; 4096]); // ring space before any partition
    for _ in 0..3 {
        partition(&mut bytes);
        append_record(&mut bytes, 6, true, &[0; 2048]);
        video(&mut bytes);
        append_record(&mut bytes, 10, true, &[0; 28]); // opaque index
        append_record(&mut bytes, 4321, false, &[1, 2, 3]); // validated unknown track
    }
    let file = parse_ubv(&mut Cursor::new(&bytes)).unwrap();
    assert!(file.read_status.is_complete());
    assert_eq!(file.read_status.validated_end_offset, bytes.len() as u64);
    assert_eq!(file.partitions.len(), 3);
    for (index, part) in file.partitions.iter().enumerate() {
        assert_eq!(part.index, index);
        assert_eq!(part.entries.len(), 3);
        assert!(matches!(part.entries[0], PartitionEntry::Untimed(ref r) if r.track_id == 6));
        assert!(matches!(part.entries[1], PartitionEntry::Frame(_)));
        assert!(matches!(part.entries[2], PartitionEntry::Untimed(ref r) if r.track_id == 10));
    }
}

fn assert_incomplete(bytes: &[u8], validated: usize, message: &str) {
    let (file, error) = parse_ubv_partial(&mut Cursor::new(bytes)).unwrap();
    assert!(!file.read_status.is_complete());
    assert_eq!(file.read_status.end_reason, EndReason::Error);
    assert_eq!(file.read_status.validated_end_offset, validated as u64);
    assert_eq!(file.read_status.file_size, bytes.len() as u64);
    assert!(error.unwrap().to_string().contains(message));
    assert!(file.read_status.failure.unwrap().message.contains(message));
    assert!(parse_ubv(&mut Cursor::new(bytes)).is_err());
}

#[test]
fn inflated_size_cannot_swallow_later_video_and_become_padding() {
    let mut bytes = Vec::new();
    partition(&mut bytes);
    let bad = video(&mut bytes);
    video(&mut bytes);
    bytes.resize(256, 0);
    bytes[bad + 12..bad + 16].copy_from_slice(&128u32.to_be_bytes());
    assert_incomplete(&bytes, bad, "back-size mismatch");
}

#[test]
fn zero_trailers_and_bad_eof_skip_trailers_are_errors() {
    let (bytes, bad) = incomplete_video();
    assert_incomplete(&bytes, bad, "back-size mismatch");
    let (file, _) = parse_ubv_partial(&mut Cursor::new(bytes)).unwrap();
    assert_eq!(file.partitions[0].entries.len(), 1);
    let failure = file.read_status.failure.unwrap();
    assert_eq!(failure.offset, bad as u64);
    assert_eq!(failure.track_id, Some(7));
    assert_eq!(failure.format_code, Some(0xF90C));

    let mut bytes = Vec::new();
    partition(&mut bytes);
    let bad = append_record(&mut bytes, 6, true, &[0; 2048]);
    let end = bytes.len();
    bytes[end - 4..].copy_from_slice(&999u32.to_be_bytes());
    assert_incomplete(&bytes, bad, "back-size mismatch");
}

#[test]
fn truncation_is_not_eof_in_either_record_api() {
    let mut bytes = Vec::new();
    partition(&mut bytes);
    let boundary = bytes.len();
    bytes.extend_from_slice(&[0xA0, 0, 7]);
    assert_incomplete(&bytes, boundary, "unexpected EOF");
    let mut cursor = Cursor::new(&bytes);
    cursor.seek(SeekFrom::Start(boundary as u64)).unwrap();
    assert!(read_record(&mut cursor).is_err());

    bytes.truncate(boundary);
    let bad = video(&mut bytes);
    bytes.truncate(bytes.len() - 2);
    assert_incomplete(&bytes, bad, "record extent exceeds input");
}

#[test]
fn skipped_large_payloads_must_fit_within_input() {
    let mut bytes = Vec::new();
    partition(&mut bytes);
    let bad = append_record(&mut bytes, 6, true, &[0; 2048]);
    bytes[bad + 8..bad + 12].copy_from_slice(&u32::MAX.to_be_bytes());
    assert_incomplete(&bytes, bad, "record extent exceeds input");
}

#[test]
fn padding_requires_the_entire_remainder_to_be_zero() {
    let mut bytes = Vec::new();
    partition(&mut bytes);
    let boundary = bytes.len();
    bytes.resize(boundary + 65539, 0);
    let file = parse_ubv(&mut Cursor::new(&bytes)).unwrap();
    assert!(file.read_status.is_complete());
    assert_eq!(file.read_status.end_reason, EndReason::ZeroPadding);
    assert_eq!(file.read_status.validated_end_offset, boundary as u64);
    assert_eq!(file.read_status.padding.unwrap().length, 65539);
    *bytes.last_mut().unwrap() = 1;
    assert_incomplete(&bytes, boundary, "nonzero byte");
    bytes.truncate(boundary + 3);
    bytes[boundary + 1] = 1;
    assert_incomplete(&bytes, boundary, "nonzero byte");
    bytes[boundary + 1] = 0;
    assert!(
        parse_ubv(&mut Cursor::new(&bytes))
            .unwrap()
            .read_status
            .is_complete()
    );
}

#[test]
fn reserved_clock_index_is_not_guessed_as_an_untimed_envelope() {
    let mut bytes = Vec::new();
    partition(&mut bytes);
    let bad = append_record(&mut bytes, 10, true, &[0; 28]);
    bytes[bad + 4] = 0xF9;
    assert_incomplete(&bytes, bad, "unsupported record layout");
}

#[test]
fn length_change_prevents_a_completeness_claim() {
    struct Growing {
        cursor: Cursor<Vec<u8>>,
        measurements: usize,
    }
    impl std::io::Read for Growing {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            std::io::Read::read(&mut self.cursor, buf)
        }
    }
    impl Seek for Growing {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            if pos == SeekFrom::End(0) {
                self.measurements += 1;
                if self.measurements == 2 {
                    self.cursor.get_mut().push(0);
                }
            }
            self.cursor.seek(pos)
        }
    }
    let mut bytes = Vec::new();
    partition(&mut bytes);
    let mut reader = Growing {
        cursor: Cursor::new(bytes),
        measurements: 0,
    };
    let (file, error) = parse_ubv_partial(&mut reader).unwrap();
    assert!(!file.read_status.is_complete());
    assert!(error.unwrap().to_string().contains("input length changed"));
}

#[test]
fn untimed_envelope_has_size_at_byte_eight_and_no_timestamp() {
    let bytes = [
        0xA0, 0, 10, 0xAA, 0xF1, 0, 0, 2, 0, 0, 0, 5, // size, without DTS
        1, 2, 3, 4, 5, 0, 0, 0, // payload and absolute alignment
        0, 0, 0, 20, // back-size excludes this trailer
    ];
    let record = read_record(&mut Cursor::new(bytes)).unwrap().unwrap();
    assert_eq!(record.dts, None);
    assert_eq!(record.clock_rate, None);
    assert_eq!(record.data_offset, 12);
    assert_eq!(record.data_size, 5);
    assert_eq!(record.sequence, 2);
    assert_eq!(record.payload, Some(vec![1, 2, 3, 4, 5]));
    assert_eq!(record.total_size, 24);
}

#[test]
fn io_failure_keeps_the_validated_prefix_and_record_context() {
    struct Failing {
        cursor: Cursor<Vec<u8>>,
        fail_at: u64,
    }
    impl std::io::Read for Failing {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.cursor.position() >= self.fail_at {
                return Err(std::io::Error::other("simulated device failure"));
            }
            std::io::Read::read(&mut self.cursor, buf)
        }
    }
    impl Seek for Failing {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.cursor.seek(pos)
        }
    }
    let mut bytes = Vec::new();
    partition(&mut bytes);
    video(&mut bytes);
    let bad = video(&mut bytes);
    let fail_at = bytes.len() as u64 - 4;
    let mut reader = Failing {
        cursor: Cursor::new(bytes),
        fail_at,
    };
    let (file, error) = parse_ubv_partial(&mut reader).unwrap();
    assert!(!file.read_status.is_complete());
    assert_eq!(file.read_status.validated_end_offset, bad as u64);
    assert_eq!(file.partitions[0].entries.len(), 1);
    let failure = file.read_status.failure.unwrap();
    assert_eq!(failure.track_id, Some(7));
    assert_eq!(failure.format_code, Some(0xF90C));
    assert!(
        error
            .unwrap()
            .to_string()
            .contains("simulated device failure")
    );
}

#[test]
fn explicit_clock_rates_must_be_nonzero_and_errors_retain_the_prefix() {
    let (bytes, bad) = zero_clock_rate();
    assert_incomplete(&bytes, bad, "invalid clock rate");
    let (file, _) = parse_ubv_partial(&mut Cursor::new(&bytes)).unwrap();
    assert_eq!(file.partitions[0].entries.len(), 1);
    let failure = file.read_status.failure.unwrap();
    assert_eq!(failure.offset, bad as u64);
    assert_eq!(failure.track_id, Some(0xDA7E));
    assert_eq!(failure.format_code, Some(0xF901));
    assert!(read_record(&mut Cursor::new(&bytes[bad..])).is_err());

    let mut valid = bytes;
    valid[bad + 8..bad + 12].copy_from_slice(&1000u32.to_be_bytes());
    let file = parse_ubv(&mut Cursor::new(valid)).unwrap();
    assert!(file.read_status.is_complete());
    assert!(
        matches!(file.partitions[0].entries[1], PartitionEntry::ClockSync(cs) if cs.sc_rate == 1000)
    );
    assert!(matches!(
        file.partitions[0].entries[2],
        PartitionEntry::Frame(_)
    ));
}
