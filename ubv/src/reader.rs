use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::Path;

use flate2::read::GzDecoder;

use crate::clock::ClockSync;
use crate::error::Result;
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
    let is_gz = path
        .to_str()
        .map(|s| s.ends_with(".gz"))
        .unwrap_or(false);

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

/// Parse a UBV file from a reader, returning all partitions and frames.
pub fn parse_ubv<R: Read + Seek>(reader: &mut R) -> Result<UbvFile> {
    let mut partitions = Vec::new();
    let mut current_partition: Option<Partition> = None;
    let mut current_clock_sync: Option<ClockSync> = None;

    while let Some(rec) = record::read_record(reader)? {
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
            }

            track::TrackType::ClockSync => {
                // Parse clock sync from payload
                if let Some(payload) = &rec.payload {
                    let cs = ClockSync::from_record(rec.dts, rec.clock_rate, payload)?;
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
