/// Clock rate lookup table indexed by sample rate index (low nibble of byte 5).
pub const CLOCK_RATES: [u32; 16] = [
    0,             // 0: reserved
    0,             // 1: special (read from stream)
    1_000,         // 2: millisecond timebase (clock sync)
    8_000,         // 3: telephony audio
    11_025,        // 4: quarter-rate audio
    12_000,        // 5: audio
    16_000,        // 6: wideband audio
    22_050,        // 7: half-rate audio
    24_000,        // 8: audio
    32_000,        // 9: audio
    44_100,        // 10: CD-quality audio
    48_000,        // 11: professional audio
    90_000,        // 12: video (RTP/MPEG-TS)
    1_000_000,     // 13: microsecond timer
    1_000_000_000, // 14: nanosecond timer
    0,             // 15: fallback/unknown
];

/// Decoded format code from bytes 4-5 of a record header.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct FormatCode(pub u16);

impl FormatCode {
    pub fn new(b4: u8, b5: u8) -> Self {
        Self(((b4 as u16) << 8) | b5 as u16)
    }

    pub fn byte4(self) -> u8 {
        (self.0 >> 8) as u8
    }

    pub fn byte5(self) -> u8 {
        self.0 as u8
    }

    /// Sample rate index (low nibble of byte 5).
    pub fn sample_rate_index(self) -> u8 {
        self.byte5() & 0x0F
    }

    /// Clock rate from the lookup table (0 if index is 0, 1, or 15).
    pub fn table_clock_rate(self) -> u32 {
        CLOCK_RATES[self.sample_rate_index() as usize]
    }

    /// Bit 7: extended header.
    pub fn extended_header(self) -> bool {
        self.byte4() & 0x80 != 0
    }

    /// Bit 5: keyframe.
    pub fn keyframe(self) -> bool {
        self.byte4() & 0x20 != 0
    }

    /// Bit 4: has CTS field.
    pub fn has_cts(self) -> bool {
        self.byte4() & 0x10 != 0
    }

    /// Bit 3: clock rate present.
    pub fn clock_rate_present(self) -> bool {
        self.byte4() & 0x08 != 0
    }

    /// Bit 2: 64-bit DTS (otherwise 32-bit).
    pub fn dts_64bit(self) -> bool {
        self.byte4() & 0x04 != 0
    }

    /// Bit 1: extra field present.
    pub fn has_extra(self) -> bool {
        self.byte4() & 0x02 != 0
    }

    /// Bit 0: extra padding.
    pub fn extra_padding(self) -> bool {
        self.byte4() & 0x01 != 0
    }

    /// Packet position from bits 7-6 (see [`PacketPosition`] for details).
    pub fn packet_position(self) -> PacketPosition {
        match (self.byte4() >> 6) & 0x03 {
            0b11 => PacketPosition::Single,
            0b10 => PacketPosition::First,
            0b01 => PacketPosition::Last,
            _ => PacketPosition::Middle,
        }
    }

    /// Compute the offset from record start to the SIZE field.
    /// This determines the header length.
    pub fn header_len(self) -> usize {
        let mut off: usize = 8; // bytes 0-7 are always present

        // If sample_rate_index == 1, read clock rate from stream (4 bytes)
        if self.sample_rate_index() == 1 {
            off += 4;
        }

        // DTS width
        if self.dts_64bit() {
            off += 8;
        } else {
            off += 4;
        }

        // Extra field
        if self.has_extra() {
            off += 4;
        }

        // Duration field is always present (when extended header is set)
        // But if bit 6 is clear, there's a separate payload_size after duration
        // The SIZE field sits at the end
        // Actually: duration is always present, then if bit6 is clear, payload_size follows
        // The "header_len" is the offset to the SIZE (payload_size) field.
        // When bit6 is set, duration IS the payload_size, so SIZE offset = off (at duration)
        // When bit6 is clear, duration is separate, then SIZE follows = off + 4

        if self.byte4() & 0x40 == 0 {
            // bit 6 clear: duration field + separate payload_size
            off += 4; // skip duration, SIZE is after it
        }
        // else: bit 6 set, duration doubles as payload_size, SIZE is at current off

        off
    }
}

/// Position of a record within a (possibly multi-packet) frame.
///
/// When a video frame is too large for a single UBV record, it is split
/// ("chunked") across multiple consecutive records on the same track.
/// Bits 7-6 of byte 4 (the high byte of the format code) encode where
/// each record sits in the sequence:
///
/// - `Single` (0b11) — the entire frame fits in one record (the common case).
/// - `First`  (0b10) — first chunk; the keyframe flag on this record applies
///   to the reassembled frame.
/// - `Middle` (0b00) — continuation chunk.
/// - `Last`   (0b01) — final chunk.
///
/// To reassemble a chunked frame, concatenate the DATA payloads from
/// First → Middle(s) → Last in file order. The per-track sequence counter
/// increments for each chunk, not each logical frame.
///
/// **Note:** All sample files observed to date contain only `Single` packets.
/// The remux demuxer does not currently implement reassembly — chunked frames
/// would produce corrupt output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub enum PacketPosition {
    Single,
    First,
    Middle,
    Last,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_code_ed0c() {
        // Video keyframe: 0xED = 1110_1101, 0x0C
        let fc = FormatCode::new(0xED, 0x0C);
        assert!(fc.keyframe());
        assert!(fc.dts_64bit());
        assert!(fc.clock_rate_present());
        assert!(fc.extra_padding());
        assert!(!fc.has_extra()); // bit1 = 0
        assert_eq!(fc.sample_rate_index(), 12);
        assert_eq!(fc.table_clock_rate(), 90_000);
        // 8 + 8(64-bit DTS) = 16. bit6=1 so duration IS payload_size.
        assert_eq!(fc.header_len(), 16);
    }

    #[test]
    fn test_format_code_f902() {
        // Clock sync old format: 0xF9 0x02
        let fc = FormatCode::new(0xF9, 0x02);
        assert!(fc.keyframe());
        assert!(!fc.dts_64bit());
        assert!(fc.clock_rate_present());
        assert!(fc.extra_padding());
        assert_eq!(fc.sample_rate_index(), 2);
        assert_eq!(fc.table_clock_rate(), 1_000);
        // 8 + 4(32-bit DTS) = 12. bit6=1 so no extra. But has_extra is bit1.
        // 0xF9 = 1111_1001. bit1=0, bit0=1.
        assert!(!fc.has_extra());
        assert_eq!(fc.header_len(), 12);
    }

    #[test]
    fn test_format_code_fd0c() {
        // Partition header / audio in new format: 0xFD = 1111_1101, 0x0C
        let fc = FormatCode::new(0xFD, 0x0C);
        assert!(fc.keyframe());
        assert!(fc.dts_64bit());
        assert!(!fc.has_extra()); // bit1 = 0
        assert!(fc.extra_padding());
        assert_eq!(fc.sample_rate_index(), 12);
        // 8 + 8(DTS) = 16. bit6=1 so duration IS payload_size. header_len = 16.
        assert_eq!(fc.header_len(), 16);
    }

    #[test]
    fn test_format_code_cd0c() {
        // Non-keyframe video: 0xCD 0x0C
        let fc = FormatCode::new(0xCD, 0x0C);
        assert!(!fc.keyframe());
        assert!(fc.dts_64bit());
        assert_eq!(fc.header_len(), 16);
    }

    #[test]
    fn test_format_code_fd02() {
        // Clock sync new format: 0xFD 0x02
        let fc = FormatCode::new(0xFD, 0x02);
        assert!(fc.dts_64bit());
        assert_eq!(fc.sample_rate_index(), 2);
        assert_eq!(fc.table_clock_rate(), 1_000);
        assert_eq!(fc.header_len(), 16);
    }

    #[test]
    fn test_format_code_fd0a() {
        // Audio (44.1kHz): 0xFD 0x0A
        let fc = FormatCode::new(0xFD, 0x0A);
        assert!(fc.keyframe());
        assert!(fc.dts_64bit());
        assert_eq!(fc.sample_rate_index(), 10);
        assert_eq!(fc.table_clock_rate(), 44_100);
        assert_eq!(fc.header_len(), 16);
    }

    #[test]
    fn test_format_code_fd06() {
        // Audio (16kHz): 0xFD 0x06
        let fc = FormatCode::new(0xFD, 0x06);
        assert_eq!(fc.table_clock_rate(), 16_000);
        assert_eq!(fc.header_len(), 16);
    }

    #[test]
    fn test_packet_position() {
        // 0xED: bits 7-6 = 11 = Single
        assert_eq!(FormatCode::new(0xED, 0x0C).packet_position(), PacketPosition::Single);
        // 0xCD: bits 7-6 = 11 = Single
        assert_eq!(FormatCode::new(0xCD, 0x0C).packet_position(), PacketPosition::Single);
    }
}
