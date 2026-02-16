use crate::format::PacketPosition;

/// Fields common to all UBV record types (media frames and metadata records).
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct RecordHeader {
    /// Track ID.
    pub track_id: u16,
    /// Byte offset of the payload data in the file.
    pub data_offset: u64,
    /// Size of the payload data in bytes.
    pub data_size: u32,
    /// Decoding timestamp in track clock rate units.
    pub dts: u64,
    /// Clock rate / timebase in Hz.
    pub clock_rate: u32,
    /// Sequence counter from the record header.
    pub sequence: u16,
    /// Whether this frame is a keyframe (from UBV format code bitflags).
    pub keyframe: bool,
}

/// A parsed media frame from the UBV file.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct Frame {
    /// Output type character: 'V' or 'A'.
    pub type_char: char,
    /// Common record header fields.
    #[serde(flatten)]
    pub header: RecordHeader,
    /// Composition timestamp offset (always 0 in observed files).
    pub cts: i64,
    /// Wall-clock time in track clock rate units.
    pub wc: u64,
    /// Position of this record within a (possibly chunked) frame.
    /// See [`PacketPosition`] for the chunking/reassembly protocol.
    /// All observed sample files contain only `Single` packets.
    pub packet_position: PacketPosition,
}
