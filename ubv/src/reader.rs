use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::Path;

use flate2::read::GzDecoder;

use crate::clock::ClockSync;
use crate::error::{Result, UbvError};
use crate::frame::{Frame, RecordHeader};
use crate::partition::{MetadataRecord, Partition, PartitionEntry, PartitionHeader};
use crate::record;
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

    loop {
        let rec = match record::read_record(reader) {
            Ok(Some(r)) => r,
            Ok(None) => break,
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

    Ok(UbvFile { partitions })
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
}
