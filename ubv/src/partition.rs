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
