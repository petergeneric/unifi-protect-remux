use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::Path;

use flate2::read::GzDecoder;

use crate::clock::ClockSync;
use crate::error::{Result, UbvError};
use crate::frame::{Frame, RecordHeader};
use crate::partition::{MetadataRecord, Partition, PartitionEntry, PartitionHeader, UntimedRecord};
use crate::record::{self, EndReason, ReadOutcome};
use crate::track;

/// A reader that transparently handles both plain `.ubv` and gzip-compressed `.ubv.gz` files.
pub enum UbvReader {
    File(BufReader<File>),
    Memory(Cursor<Vec<u8>>),
}

impl Read for UbvReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            UbvReader::File(r) => r.read(buf),
            UbvReader::Memory(r) => r.read(buf),
        }
    }
}

impl Seek for UbvReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self {
            UbvReader::File(r) => r.seek(pos),
            UbvReader::Memory(r) => r.seek(pos),
        }
    }
}

/// Open a `.ubv` or `.ubv.gz` file and return a seekable reader.
///
/// Gzip-compressed files are fully decompressed into memory.
///
/// This is intentional: in this project `.ubv.gz` parsing is primarily used by
/// unit/integration tests and fixture tooling. Production remux/anonymise flows
/// operate on plain `.ubv` files, so we prefer the simplest seekable approach
/// here over adding a more complex seekable-gzip implementation.
pub fn open_ubv(path: &Path) -> std::io::Result<UbvReader> {
    let is_gz = path.to_str().map(|s| s.ends_with(".gz")).unwrap_or(false);

    if is_gz {
        let file = File::open(path)?;
        let mut decoder = GzDecoder::new(file);
        let mut buf = Vec::new();
        // Keep gzip handling simple and fully seekable for test fixtures.
        decoder.read_to_end(&mut buf)?;
        Ok(UbvReader::Memory(Cursor::new(buf)))
    } else {
        let file = File::open(path)?;
        Ok(UbvReader::File(BufReader::new(file)))
    }
}

/// Parsed UBV file contents.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct UbvFile {
    pub partitions: Vec<Partition>,
    /// Where and why reading stopped, and the stream length.
    pub read_status: ReadStatus,
}

/// How far parsing got. A complete file has `end_offset == file_size` and
/// `end_reason == Eof`; `ZeroPadding` before the end is normal for a
/// preallocated file or one still being written; anything else means the tail
/// was not read.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct ReadStatus {
    /// Length of the stream in bytes.
    pub file_size: u64,
    /// Offset at which reading stopped (end of the last record read, or start
    /// of the zero padding / incomplete record).
    pub end_offset: u64,
    /// Why reading stopped.
    pub end_reason: EndReason,
}

/// Snapshot of the most recently parsed record. Used to enrich errors raised
/// by the *next* `read_record` call.
#[derive(Debug, Clone, Copy)]
struct PrevRecord {
    offset: u64,
    track_id: u16,
    total_size: u64,
}

/// Bookkeeping for the partition currently being assembled. `record_count`
/// includes the partition-header record itself, so it starts at 1 when a
/// new partition is opened.
#[derive(Debug, Clone, Copy)]
struct PartitionState {
    index: usize,
    record_count: usize,
}

/// Parse a UBV file from a reader, returning all partitions and frames.
pub fn parse_ubv<R: Read + Seek>(reader: &mut R) -> Result<UbvFile> {
    let mut partitions = Vec::new();
    let mut current_partition: Option<Partition> = None;
    let mut current_clock_sync: Option<ClockSync> = None;

    // State captured for error decoration. `prev_record` is the last record
    // we parsed cleanly; comparing `prev.offset + prev.total_size` to the
    // failure offset reveals mis-sized prior records — the typical root
    // cause of bad-magic / checksum failures downstream.
    let mut prev_record: Option<PrevRecord> = None;
    let mut partition_state: Option<PartitionState> = None;
    let mut total_records: u64 = 0;

    let wrap = |e: UbvError,
                prev: Option<PrevRecord>,
                ps: Option<PartitionState>,
                total: u64|
     -> UbvError {
        UbvError::ParseContext {
            state: format_parser_state(prev, ps, total),
            source: Box::new(e),
        }
    };

    let file_size = stream_len(reader)?;
    let (end_reason, end_offset);

    loop {
        let rec = match record::read_record_outcome(reader) {
            Ok(ReadOutcome::Record(r)) => r,
            Ok(ReadOutcome::End(reason, offset)) => {
                (end_reason, end_offset) = (reason, offset);
                break;
            }
            Err(e) => return Err(wrap(e, prev_record, partition_state, total_records)),
        };

        prev_record = Some(PrevRecord {
            offset: rec.file_offset,
            track_id: rec.track_id,
            total_size: rec.total_size,
        });
        total_records += 1;
        // PartitionHeader records reset this counter below; for every other
        // record, count it against the current partition (if any).
        if let Some(ps) = partition_state.as_mut() {
            ps.record_count += 1;
        }

        // Untimed records (no DTS) are kept as their own entries, whatever the
        // track: they are neither frames nor timed metadata.
        if !rec.has_dts {
            if let Some(p) = current_partition.as_mut() {
                p.entries.push(PartitionEntry::Untimed(UntimedRecord {
                    track_id: rec.track_id,
                    format_code: rec.format_code,
                    sequence: rec.sequence,
                    file_offset: rec.file_offset,
                    data_offset: rec.data_offset,
                    data_size: rec.data_size,
                }));
            }
            continue;
        }

        let info = match track::track_info(rec.track_id) {
            Some(i) => i,
            None => continue, // Unknown track, skip
        };

        let header = RecordHeader {
            track_id: rec.track_id,
            data_offset: rec.data_offset,
            data_size: rec.data_size,
            dts: rec.dts,
            clock_rate: rec.clock_rate,
            sequence: rec.sequence,
            keyframe: rec.format_code.keyframe(),
        };

        match info.track_type {
            track::TrackType::PartitionHeader => {
                // Start a new partition
                let idx = partitions.len();
                if let Some(p) = current_partition.take() {
                    partitions.push(p);
                }
                let ph = rec.payload.as_ref().map(|payload| PartitionHeader {
                    file_offset: rec.file_offset,
                    dts: rec.dts,
                    clock_rate: rec.clock_rate,
                    format_code: rec.format_code,
                    payload: payload.clone(),
                });
                current_partition = Some(Partition {
                    index: idx,
                    entries: Vec::new(),
                    header: ph,
                });
                current_clock_sync = None;
                partition_state = Some(PartitionState {
                    index: idx,
                    record_count: 1, // the header is the partition's first record
                });
            }

            track::TrackType::ClockSync => {
                // Parse clock sync from payload
                if let Some(payload) = &rec.payload {
                    let cs = ClockSync::from_record(
                        rec.dts,
                        rec.clock_rate,
                        rec.file_offset,
                        rec.track_id,
                        payload,
                    )
                    .map_err(|e| wrap(e, prev_record, partition_state, total_records))?;
                    current_clock_sync = Some(cs);

                    if let Some(p) = current_partition.as_mut() {
                        p.entries.push(PartitionEntry::ClockSync(cs));
                    }
                }
            }

            _ if track::is_media_track(rec.track_id) => {
                let type_char = info.type_char.unwrap_or('?');

                // Compute wall-clock if we have a clock sync
                let wc = match &current_clock_sync {
                    Some(cs) => cs.compute_wall_clock(rec.dts, rec.clock_rate),
                    None => 0,
                };

                let frame = Frame {
                    type_char,
                    header,
                    cts: 0,
                    wc,
                    packet_position: rec.format_code.packet_position(),
                };

                if let Some(p) = current_partition.as_mut() {
                    p.entries.push(PartitionEntry::Frame(frame));
                }
            }

            track::TrackType::Motion
            | track::TrackType::SmartEvent
            | track::TrackType::Jpeg
            | track::TrackType::Skip
            | track::TrackType::Talkback => {
                let meta = MetadataRecord {
                    header,
                    file_offset: rec.file_offset,
                };

                let entry = match info.track_type {
                    track::TrackType::Motion => PartitionEntry::Motion(meta),
                    track::TrackType::SmartEvent => PartitionEntry::SmartEvent(meta),
                    track::TrackType::Jpeg => PartitionEntry::Jpeg(meta),
                    track::TrackType::Skip => PartitionEntry::Skip(meta),
                    track::TrackType::Talkback => PartitionEntry::Talkback(meta),
                    _ => unreachable!(),
                };

                if let Some(p) = current_partition.as_mut() {
                    p.entries.push(entry);
                }
            }

            _ => {
                // Reserved or other non-media tracks — skip
            }
        }
    }

    // Push the last partition
    if let Some(p) = current_partition {
        partitions.push(p);
    }

    Ok(UbvFile {
        partitions,
        read_status: ReadStatus {
            file_size,
            end_offset,
            end_reason,
        },
    })
}

/// Length of the stream, leaving the position where it was.
fn stream_len<R: Seek>(reader: &mut R) -> Result<u64> {
    let io = |e| UbvError::Io {
        context: "measuring stream length",
        source: e,
    };
    let pos = reader.stream_position().map_err(io)?;
    let len = reader.seek(SeekFrom::End(0)).map_err(io)?;
    reader.seek(SeekFrom::Start(pos)).map_err(io)?;
    Ok(len)
}

/// Render the parser's state at the moment a parse step failed, for
/// inclusion in `UbvError::ParseContext`. The previous-record summary is the
/// most useful diagnostic clue: the next record should start at
/// `prev.offset + prev.total_size`, so a mismatch with the failure offset
/// indicates a mis-sized prior record.
fn format_parser_state(
    prev_record: Option<PrevRecord>,
    partition_state: Option<PartitionState>,
    total_records: u64,
) -> String {
    let partition_desc = match partition_state {
        Some(PartitionState {
            index,
            record_count,
        }) => format!("inside partition #{index} ({record_count} record(s) in)"),
        None => "before any partition header".to_string(),
    };

    let prev_desc = match prev_record {
        Some(PrevRecord {
            offset,
            track_id,
            total_size,
        }) => {
            let expected_next = offset.saturating_add(total_size);
            format!(
                "previous record at offset 0x{offset:X} (track 0x{track_id:04X} / {name}, total_size={total_size}, next expected at 0x{expected_next:X})",
                name = track::track_display_name(track_id),
            )
        }
        None => "no records read yet".to_string(),
    };

    format!("{partition_desc}; {prev_desc}; {total_records} total record(s) read")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::UbvError;
    use std::io::Cursor;

    /// A bad magic byte after a valid record should produce a `ParseContext`
    /// error wrapping the original `BadMagic`, with the prior record's offset
    /// and track id surfaced in the message.
    #[test]
    fn bad_magic_after_valid_record_includes_parser_state() {
        // Reuse the partition-header byte sequence from `record::tests` and
        // append a single bogus byte where the next record's magic would be.
        let mut data: Vec<u8> = vec![
            0xa0, 0x00, 0x09, 0xa9, 0xfd, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x17, 0xde, 0xc4,
            0x98, 0xab, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x3f, 0xf9, 0xec, 0x70,
            0x64, 0x5d, 0xc6, 0x17, 0x02, 0x68, 0x03, 0x03, 0xe4, 0x00, 0x28, 0xdd, 0x00, 0x00,
            0x00, 0x28,
        ];
        let valid_len = data.len() as u64;
        data.extend_from_slice(&[0xBA, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

        let mut cursor = Cursor::new(data);
        let err = parse_ubv(&mut cursor).expect_err("should fail on bad magic");

        let UbvError::ParseContext { state, source } = err else {
            panic!("expected ParseContext, got {err:?}");
        };

        assert!(matches!(
            *source,
            UbvError::BadRecordMagic { got: 0xBA, .. }
        ));
        assert!(
            state.contains("partition #0"),
            "state missing partition index: {state}"
        );
        assert!(
            state.contains(&format!("next expected at 0x{valid_len:X}")),
            "state missing expected-next offset: {state}"
        );
        assert!(
            state.contains(&format!("total_size={valid_len}")),
            "state missing total_size of previous record: {state}"
        );
        assert!(
            state.contains("track 0x0009"),
            "state missing previous track id: {state}"
        );
    }

    /// A malformed clock-sync payload is propagated through the same wrapper,
    /// so callers see consistent parser-state context regardless of which
    /// step inside the parse loop failed.
    #[test]
    fn clock_sync_parse_failure_includes_parser_state() {
        // Valid record envelope, but DATA is too short for ClockSync::from_record
        // (which requires at least 8 bytes: u32 seconds + u32 nanoseconds).
        let data: Vec<u8> = vec![
            // Tag (track 0xDA7E = clock sync), checksum byte = a0^da^7e = 04
            0xa0, 0xda, 0x7e, 0x04, // Format code F9 02, seq 00 00
            0xf9, 0x02, 0x00, 0x00, // DTS 32-bit
            0x43, 0xe5, 0xbd, 0x6e, // SIZE = 4 (too small for clock-sync payload)
            0x00, 0x00, 0x00, 0x04, // DATA (4 bytes)
            0x64, 0x5d, 0xc6, 0x12, // BACK_SIZE = 20 (12+4+4+0)
            0x00, 0x00, 0x00, 0x14,
        ];
        let mut cursor = Cursor::new(data);
        let err = parse_ubv(&mut cursor).expect_err("should fail on short clock sync");

        let UbvError::ParseContext { state, source } = err else {
            panic!("expected ParseContext, got {err:?}");
        };
        assert!(matches!(*source, UbvError::ShortPayload { .. }));
        assert!(
            state.contains("previous record"),
            "state missing previous-record summary: {state}"
        );
    }

    use crate::partition::PartitionEntry;
    use crate::test_util::{UbvBuilder, index_payload};

    /// Two partitions, each closed by an untimed index record on track 10 whose
    /// payload starts with zero bytes, then an untimed Skip that runs to the end
    /// of the file. All partitions must be read.
    fn two_partition_file() -> (UbvBuilder, Vec<u64>) {
        let mut b = UbvBuilder::default();
        let mut untimed_offsets = Vec::new();
        for (p, base_secs) in [(0u64, 1_700_000_000u32), (1, 1_700_000_100)] {
            b.partition_header(p);
            b.clock_sync(p * 1_000_000, base_secs);
            for i in 0..3u16 {
                b.video(
                    i,
                    p * 90_000_000 + i as u64 * 3_000,
                    i == 0,
                    10 + i as usize,
                );
            }
            untimed_offsets.push(b.untimed(10, 0, &index_payload(&[(base_secs as u64 * 1000, 0)])));
        }
        untimed_offsets.push(b.untimed(6, 0, &[0u8; 1500]));
        (b, untimed_offsets)
    }

    #[test]
    fn untimed_records_do_not_stop_parsing() {
        let (b, untimed_offsets) = two_partition_file();
        let mut cursor = Cursor::new(b.bytes);
        let ubv = parse_ubv(&mut cursor).unwrap();

        assert_eq!(ubv.partitions.len(), 2);
        for (n, p) in ubv.partitions.iter().enumerate() {
            let frames: Vec<_> = p
                .entries
                .iter()
                .filter_map(|e| match e {
                    PartitionEntry::Frame(f) => Some(f),
                    _ => None,
                })
                .collect();
            assert_eq!(frames.len(), 3, "partition {n}");
            // Wall clock follows the partition's own clock sync.
            assert_eq!(frames[0].wc / 90, 1_700_000_000_000 + n as u64 * 100_000);

            let untimed: Vec<_> = p
                .entries
                .iter()
                .filter_map(|e| match e {
                    PartitionEntry::Untimed(u) => Some(u),
                    _ => None,
                })
                .collect();
            // No untimed record is misreported as a smart event or skip.
            assert!(
                !p.entries
                    .iter()
                    .any(|e| matches!(e, PartitionEntry::SmartEvent(_) | PartitionEntry::Skip(_)))
            );
            let expected: Vec<u16> = if n == 0 { vec![10] } else { vec![10, 6] };
            assert_eq!(
                untimed.iter().map(|u| u.track_id).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(untimed[0].file_offset, untimed_offsets[n]);
            assert_eq!(untimed[0].data_offset, untimed_offsets[n] + 12);
            assert_eq!(untimed[0].data_size, 8 + 16);
        }
        let skip = match ubv.partitions[1].entries.last() {
            Some(PartitionEntry::Untimed(u)) => u,
            other => panic!("expected trailing untimed skip, got {other:?}"),
        };
        assert_eq!(skip.file_offset, untimed_offsets[2]);
        assert_eq!(skip.data_size, 1500);
    }

    #[test]
    fn trailing_zero_padding_after_untimed_records_is_eof() {
        let (mut b, _) = two_partition_file();
        b.bytes.extend_from_slice(&[0u8; 4096]);
        let mut cursor = Cursor::new(b.bytes);
        assert_eq!(parse_ubv(&mut cursor).unwrap().partitions.len(), 2);
    }

    /// A file still being written ends with a Skip reserved up to the end of the
    /// file whose trailing back-size is not final yet: it is the last record
    /// (nothing follows it) and is read as such.
    #[test]
    fn end_of_file_skip_with_unfinished_back_size() {
        let mut b = UbvBuilder::default();
        b.partition_header(0);
        b.clock_sync(0, 1_700_000_000);
        b.video(0, 0, true, 40);
        let skip = b.untimed(6, 0, &[0u8; 4000]);
        let n = b.bytes.len();
        b.bytes[n - 4..].copy_from_slice(&123_456u32.to_be_bytes()); // stale back-size
        let ubv = parse_ubv(&mut Cursor::new(b.bytes.clone())).unwrap();
        match ubv.partitions[0].entries.last() {
            Some(PartitionEntry::Untimed(u)) => {
                assert_eq!((u.track_id, u.file_offset, u.data_size), (6, skip, 4000))
            }
            other => panic!("{other:?}"),
        }

        // The same Skip with something after it is not the end of the file: the
        // stale back-size is not trusted, and the record is a back-size error.
        b.bytes.extend_from_slice(&[0u8; 64]);
        let err = parse_ubv(&mut Cursor::new(b.bytes)).unwrap_err();
        assert!(
            matches!(source_of(err), UbvError::BackSizeMismatch { .. }),
            "unexpected error"
        );
    }

    fn frame_count(p: &Partition) -> usize {
        p.entries
            .iter()
            .filter(|e| matches!(e, PartitionEntry::Frame(_)))
            .count()
    }

    fn source_of(err: UbvError) -> UbvError {
        match err {
            UbvError::ParseContext { source, .. } => *source,
            other => panic!("expected ParseContext, got {other:?}"),
        }
    }

    /// A zero byte where a record should start, followed by more data, is an
    /// error with its offset instead of a silent end of file.
    #[test]
    fn zero_byte_mid_file_is_an_error_with_offset() {
        let mut b = UbvBuilder::default();
        b.partition_header(0);
        b.clock_sync(0, 1_700_000_000);
        b.video(0, 0, true, 10);
        let gap = b.offset();
        b.bytes.extend_from_slice(&[0u8; 4]);
        b.video(1, 3_000, false, 10);
        let mut cursor = Cursor::new(b.bytes);

        let err = parse_ubv(&mut cursor).expect_err("should fail on zero byte mid-file");
        let UbvError::ParseContext { state, source } = err else {
            panic!("expected ParseContext, got {err:?}");
        };
        let UbvError::ZeroByteNotPadding {
            offset,
            nonzero_offset,
        } = *source
        else {
            panic!("expected ZeroByteNotPadding, got {source:?}");
        };
        assert_eq!(offset, gap);
        assert_eq!(nonzero_offset, gap + 4);
        assert!(
            state.contains(&format!("next expected at 0x{gap:X}")),
            "{state}"
        );
    }

    /// A trailing untimed Skip cut short no longer fits the untimed layout, and
    /// its timed reading has a wrong trailer. If what is left is all zeros (as
    /// here: a zero-filled Skip), parsing ends cleanly before the Skip; with
    /// non-zero data it is an error (it cannot be told from corruption mid-file).
    #[test]
    fn cut_trailing_untimed_skip() {
        let (mut b, _) = two_partition_file();
        b.bytes.truncate(b.bytes.len() - 10);
        let ubv = parse_ubv(&mut Cursor::new(b.bytes.clone())).unwrap();
        assert_eq!(ubv.partitions.len(), 2);
        assert!(matches!(
            ubv.partitions[1].entries.last(),
            Some(PartitionEntry::Untimed(u)) if u.track_id == 10
        ));

        let last = b.bytes.len() - 1;
        b.bytes[last] = 0x07; // non-zero data in the Skip: an error
        let err = parse_ubv(&mut Cursor::new(b.bytes)).unwrap_err();
        assert!(
            matches!(source_of(err), UbvError::BackSizeMismatch { .. }),
            "unexpected error"
        );
    }

    /// A preallocated file copied while being written: records, then one whose
    /// back-size is still zero, then only zeros. Clean end before that record,
    /// keeping everything read.
    #[test]
    fn record_cut_by_preallocated_zeros_is_a_clean_end() {
        let mut b = UbvBuilder::default();
        b.partition_header(0);
        b.clock_sync(0, 1_700_000_000);
        b.video(0, 0, true, 40);
        b.video(1, 3_000, false, 40);
        b.video(2, 6_000, false, 40);
        let n = b.bytes.len();
        b.bytes[n - 4..].fill(0); // its back-size: not written yet
        b.bytes.extend_from_slice(&[0u8; 8192]);
        let ubv = parse_ubv(&mut Cursor::new(b.bytes.clone())).unwrap();
        assert_eq!(frame_count(&ubv.partitions[0]), 2);

        // Only the first byte of the next header written, then zeros.
        let mut c = UbvBuilder::default();
        c.partition_header(0);
        c.clock_sync(0, 1_700_000_000);
        c.video(0, 0, true, 40);
        c.bytes.push(0xA0);
        c.bytes.extend_from_slice(&[0u8; 4095]);
        let ubv = parse_ubv(&mut Cursor::new(c.bytes)).unwrap();
        assert_eq!(frame_count(&ubv.partitions[0]), 1);

        // Zeros and then data again: not padding, still an error.
        let mut d = b.bytes.clone();
        let last = d.len() - 1;
        d[last] = 0x01;
        let err = parse_ubv(&mut Cursor::new(d)).unwrap_err();
        assert!(
            matches!(source_of(err), UbvError::BackSizeMismatch { got: 0, .. }),
            "unexpected error"
        );
    }

    /// A corrupt, non-zero back-size is still an error.
    #[test]
    fn corrupt_nonzero_back_size_is_still_an_error() {
        let mut b = UbvBuilder::default();
        b.partition_header(0);
        b.clock_sync(0, 1_700_000_000);
        b.video(0, 0, true, 40);
        let n = b.bytes.len();
        b.bytes[n - 1] ^= 0x10;
        b.bytes.extend_from_slice(&[0u8; 4096]);
        let err = parse_ubv(&mut Cursor::new(b.bytes)).unwrap_err();
        assert!(
            matches!(source_of(err), UbvError::BackSizeMismatch { .. }),
            "unexpected error"
        );
    }

    #[test]
    fn read_status_complete_file() {
        let (b, _) = two_partition_file();
        let len = b.offset();
        let mut cursor = Cursor::new(b.bytes);
        let st = parse_ubv(&mut cursor).unwrap().read_status;
        assert_eq!(
            (st.file_size, st.end_offset, st.end_reason),
            (len, len, EndReason::Eof)
        );
    }

    #[test]
    fn read_status_zero_padding_and_truncation() {
        let (mut b, _) = two_partition_file();
        let len = b.offset();
        b.bytes.extend_from_slice(&[0u8; 4096]);
        let mut cursor = Cursor::new(b.bytes);
        let st = parse_ubv(&mut cursor).unwrap().read_status;
        assert_eq!(
            (st.file_size, st.end_offset, st.end_reason),
            (len + 4096, len, EndReason::ZeroPadding)
        );

        // Cut inside a large video frame after the last index record.
        let (mut b, _) = two_partition_file();
        let frame = b.video(9, 999_999, false, 5_000);
        b.bytes.truncate(b.bytes.len() - 10);
        let len = b.offset();
        let mut cursor = Cursor::new(b.bytes);
        let ubv = parse_ubv(&mut cursor).unwrap();
        assert_eq!(ubv.partitions.len(), 2);
        let st = ubv.read_status;
        assert_eq!(st.file_size, len);
        assert_eq!(st.end_offset, frame);
        assert_eq!(st.end_reason, EndReason::TruncatedRecord);
    }

    /// The clean ends for a file copied while being written (trailing Skip cut
    /// short, record cut by preallocated zeros) are reported as `ZeroPadding`
    /// at the offset of the record that was not read.
    #[test]
    fn read_status_for_preallocated_tails() {
        let (mut b, untimed) = two_partition_file();
        b.bytes.truncate(b.bytes.len() - 10);
        let st = parse_ubv(&mut Cursor::new(b.bytes)).unwrap().read_status;
        assert_eq!(
            (st.end_reason, st.end_offset),
            (EndReason::ZeroPadding, untimed[2])
        );

        let mut c = UbvBuilder::default();
        c.partition_header(0);
        c.clock_sync(0, 1_700_000_000);
        c.video(0, 0, true, 40);
        let cut = c.video(1, 3_000, false, 40);
        let n = c.bytes.len();
        c.bytes[n - 4..].fill(0);
        c.bytes.extend_from_slice(&[0u8; 8192]);
        let st = parse_ubv(&mut Cursor::new(c.bytes)).unwrap().read_status;
        assert_eq!(
            (st.end_reason, st.end_offset),
            (EndReason::ZeroPadding, cut)
        );
    }
}
