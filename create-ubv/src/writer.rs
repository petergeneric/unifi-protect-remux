//! UBV record writer — the inverse of `ubv::record::read_record`.
//!
//! Emits byte-level-accurate records that round-trip through the parser.
//! The byte layout is exactly what `read_record` expects:
//!
//! ```text
//! byte 0:      0xA0 magic
//! bytes 1-2:   track ID (u16 BE)
//! byte 3:      XOR checksum: byte0 ^ byte1 ^ byte2
//! bytes 4-5:   format code
//! bytes 6-7:   sequence (u16 BE)
//! (optional)   clock_rate u32 BE       -- only if SRI == 1
//! DTS          u64 BE or u32 BE        -- depending on bit 2 of byte 4
//! (optional)   extra u32 BE            -- only if bit 1 of byte 4 set
//! (optional)   duration u32 BE         -- only if bit 6 of byte 4 CLEAR
//! SIZE:        u32 BE (payload length)
//! payload:     `data_size` bytes
//! padding:     0..3 bytes so the total before BACK_SIZE is 4-byte aligned
//! BACK_SIZE:   u32 BE = header_len + 4 + data_size + pad
//! ```

use std::io::{self, Write};

/// Builder for a single UBV record. Fill in the fields, then call `encode()`
/// to get the byte sequence ready to write to the output file.
#[derive(Debug, Clone)]
pub struct Record<'a> {
    pub track_id: u16,
    /// Byte 4 of the format code. Determines header layout.
    pub byte4: u8,
    /// Byte 5 of the format code. Low nibble is the sample-rate index (SRI).
    pub byte5: u8,
    pub sequence: u16,
    pub dts: u64,
    /// Optional clock rate (only written when SRI == 1).
    pub clock_rate_in_stream: Option<u32>,
    /// Optional extra field (written when bit 1 of byte 4 is set).
    pub extra: Option<u32>,
    /// Optional duration field (written when bit 6 of byte 4 is CLEAR).
    pub duration: Option<u32>,
    pub payload: &'a [u8],
}

impl<'a> Record<'a> {
    /// Compute the offset-to-SIZE (header length) for this record.
    /// Mirrors `FormatCode::header_len()` exactly.
    fn header_len(&self) -> usize {
        let mut off: usize = 8;
        if self.byte5 & 0x0F == 1 {
            off += 4;
        }
        if self.byte4 & 0x04 != 0 {
            off += 8;
        } else {
            off += 4;
        }
        if self.byte4 & 0x02 != 0 {
            off += 4;
        }
        if self.byte4 & 0x40 == 0 {
            off += 4;
        }
        off
    }

    /// Serialise into `out`, starting at the given absolute file offset
    /// (needed for the alignment padding calculation). Returns total bytes written.
    pub fn write_to<W: Write>(&self, out: &mut W, file_offset: u64) -> io::Result<u64> {
        let header_len = self.header_len();
        let data_size = self.payload.len() as u32;

        // Padding to 4-byte alignment, matching read_record's alignment_padding.
        let unpadded = file_offset + header_len as u64 + 4 + data_size as u64;
        let pad = ((4 - (unpadded % 4)) % 4) as u32;

        let back_size = header_len as u32 + 4 + data_size + pad;
        let total = back_size as u64 + 4;

        // Bytes 0-3: magic + track + XOR checksum
        let t_hi = (self.track_id >> 8) as u8;
        let t_lo = self.track_id as u8;
        let checksum = 0xA0u8 ^ t_hi ^ t_lo;
        out.write_all(&[0xA0, t_hi, t_lo, checksum])?;

        // Bytes 4-7: format + sequence
        out.write_all(&[self.byte4, self.byte5])?;
        out.write_all(&self.sequence.to_be_bytes())?;

        // Optional clock_rate (SRI == 1)
        if self.byte5 & 0x0F == 1 {
            let cr = self.clock_rate_in_stream.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "SRI==1 requires clock_rate_in_stream",
                )
            })?;
            out.write_all(&cr.to_be_bytes())?;
        }

        // DTS (64- or 32-bit)
        if self.byte4 & 0x04 != 0 {
            out.write_all(&self.dts.to_be_bytes())?;
        } else {
            out.write_all(&(self.dts as u32).to_be_bytes())?;
        }

        // Extra (bit 1)
        if self.byte4 & 0x02 != 0 {
            let e = self.extra.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "byte4 bit 1 set requires extra field",
                )
            })?;
            out.write_all(&e.to_be_bytes())?;
        }

        // Duration (bit 6 clear)
        if self.byte4 & 0x40 == 0 {
            let d = self.duration.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "byte4 bit 6 clear requires duration field",
                )
            })?;
            out.write_all(&d.to_be_bytes())?;
        }

        // SIZE
        out.write_all(&data_size.to_be_bytes())?;

        // Payload + padding + back_size
        out.write_all(self.payload)?;
        if pad > 0 {
            let zeros = [0u8; 3];
            out.write_all(&zeros[..pad as usize])?;
        }
        out.write_all(&back_size.to_be_bytes())?;

        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use ubv::record::read_record;

    /// A simple video frame record (byte4=0xFD: 64-bit DTS, no extra, bit 6 set
    /// so duration IS payload_size) at 90 kHz should round-trip cleanly.
    #[test]
    fn video_frame_roundtrips() {
        let payload = b"hello NAL";
        let rec = Record {
            track_id: 7,
            byte4: 0xFD,
            byte5: 0x0C,
            sequence: 1,
            dts: 12345,
            clock_rate_in_stream: None,
            extra: None,
            duration: None,
            payload,
        };

        let mut buf = Vec::new();
        rec.write_to(&mut buf, 0).unwrap();

        let mut cur = Cursor::new(buf);
        let parsed = read_record(&mut cur).unwrap().unwrap();
        assert_eq!(parsed.track_id, 7);
        assert_eq!(parsed.dts, 12345);
        assert_eq!(parsed.clock_rate, 90_000);
        assert_eq!(parsed.data_size, payload.len() as u32);
        assert_eq!(parsed.payload.as_deref(), Some(&payload[..]));
    }

    /// Clock-sync-shaped record (byte4=0xF9: 32-bit DTS, bit 6 set, SRI=2).
    /// Matches the reader's `test_parse_clock_sync_old` byte layout.
    #[test]
    fn clock_sync_roundtrips() {
        let payload: &[u8] = &[
            0x64, 0x5d, 0xc6, 0x12, // seconds
            0x34, 0xed, 0xce, 0x00, // nanoseconds
        ];
        let rec = Record {
            track_id: 0xDA7E,
            byte4: 0xF9,
            byte5: 0x02,
            sequence: 0,
            dts: 0x43E5BD6E,
            clock_rate_in_stream: None,
            extra: None,
            duration: None,
            payload,
        };

        let mut buf = Vec::new();
        let total = rec.write_to(&mut buf, 0).unwrap();
        // 12-byte header + 4-byte SIZE + 8-byte payload + 0 pad + 4-byte back = 28
        assert_eq!(total, 28);

        let mut cur = Cursor::new(buf);
        let parsed = read_record(&mut cur).unwrap().unwrap();
        assert_eq!(parsed.track_id, 0xDA7E);
        assert_eq!(parsed.dts, 0x43E5BD6E);
        assert_eq!(parsed.clock_rate, 1000);
        assert_eq!(parsed.data_size, 8);
    }

    /// Record with bit 6 CLEAR, so the writer must emit a separate duration
    /// field before the SIZE field.
    #[test]
    fn record_with_duration_field_roundtrips() {
        let payload = b"frame data";
        let rec = Record {
            track_id: 7,
            // bit 7=1, bit 6=0 (duration field present), bit 5=0 (non-key),
            // bit 4=0, bit 3=1 (clock rate), bit 2=1 (64-bit DTS), bit 1=0, bit 0=1.
            byte4: 0b1000_1101,
            byte5: 0x0C,
            sequence: 42,
            dts: 99999,
            clock_rate_in_stream: None,
            extra: None,
            duration: Some(3000),
            payload,
        };

        let mut buf = Vec::new();
        rec.write_to(&mut buf, 0).unwrap();

        let mut cur = Cursor::new(buf);
        let parsed = read_record(&mut cur).unwrap().unwrap();
        assert_eq!(parsed.dts, 99999);
        assert!(!parsed.format_code.keyframe());
        assert_eq!(parsed.duration, Some(3000));
        assert_eq!(parsed.data_size, payload.len() as u32);
    }

    /// Writing at a non-zero file offset must adjust alignment padding correctly.
    #[test]
    fn padding_respects_file_offset() {
        // payload of length 1: total without pad is header_len + 4 + 1.
        // header_len for 0xFD/0x0C = 16. unpadded = offset + 21.
        // At offset 1: 22 % 4 = 2, pad = 2.
        let rec = Record {
            track_id: 7,
            byte4: 0xFD,
            byte5: 0x0C,
            sequence: 0,
            dts: 1,
            clock_rate_in_stream: None,
            extra: None,
            duration: None,
            payload: &[0xAB],
        };

        let mut buf = vec![0u8; 1]; // pretend there's one byte before
        let total = rec.write_to(&mut buf, 1).unwrap();
        // header(16) + SIZE(4) + data(1) + pad(2) + back_size(4) = 27
        assert_eq!(total, 27);
        assert_eq!(buf.len(), 1 + 27);

        // Seek past the prefix byte and parse.
        let mut cur = Cursor::new(&buf[1..]);
        let parsed = read_record(&mut cur).unwrap().unwrap();
        assert_eq!(parsed.payload.as_deref(), Some(&[0xAB][..]));
    }
}
