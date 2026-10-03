use crate::clock::ClockSync;
use crate::format::FormatCode;
use crate::frame::{Frame, RecordHeader};

/// Parsed partition header record metadata.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct PartitionHeader {
    /// Absolute byte offset of the partition header record in the file.
    pub file_offset: u64,
    /// DTS from the partition header record.
    pub dts: u64,
    /// Clock rate from the partition header record.
    pub clock_rate: u32,
    /// Format code from the partition header record.
    pub format_code: FormatCode,
    /// Raw payload bytes from the partition header record.
    pub payload: Vec<u8>,
}

/// A non-media record (motion, smart event, JPEG, skip, talkback).
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct MetadataRecord {
    /// Common record header fields.
    #[serde(flatten)]
    pub header: RecordHeader,
    /// Absolute byte offset of this record in the file.
    pub file_offset: u64,
}

/// A record without a DTS field (clock index 0, e.g. format code `F1 00`).
///
/// Seen on track 10 at the end of every partition (firmware from around May
/// 2026), where the payload looks like an index of (wall-clock ms, file offset)
/// pairs, and on track 6 (Skip) at the end of the file. The payload is not
/// decoded here; read it from `data_offset` / `data_size`.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct UntimedRecord {
    /// Track ID.
    pub track_id: u16,
    /// Format code (bytes 4-5 of the record).
    pub format_code: FormatCode,
    /// Sequence counter from the record header.
    pub sequence: u16,
    /// Absolute byte offset of this record in the file.
    pub file_offset: u64,
    /// Absolute byte offset of the payload data in the file.
    pub data_offset: u64,
    /// Size of the payload data in bytes.
    pub data_size: u32,
}

/// An event in the partition's stream.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub enum PartitionEntry {
    ClockSync(ClockSync),
    Frame(Frame),
    Motion(MetadataRecord),
    SmartEvent(MetadataRecord),
    Jpeg(MetadataRecord),
    Skip(MetadataRecord),
    Talkback(MetadataRecord),
    /// Record without DTS (see [`UntimedRecord`]); never a media frame.
    Untimed(UntimedRecord),
}

/// A partition (recording segment) within a UBV file.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct Partition {
    /// Partition index (0-based).
    pub index: usize,
    /// Entries in file order (clock syncs interleaved with frames and metadata).
    pub entries: Vec<PartitionEntry>,
    /// Parsed partition header record, if present.
    pub header: Option<PartitionHeader>,
}
