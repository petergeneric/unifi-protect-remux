use std::io::{Read, Seek, SeekFrom};

use crate::error::{Result, UbvError};
use crate::format::FormatCode;

/// Maximum payload size (in bytes) that is read inline during record parsing.
/// Records with payloads up to this size have their data captured in memory;
/// larger payloads are skipped and must be read separately via data_offset.
const MAX_INLINE_PAYLOAD: u32 = 1024;

/// A parsed record envelope from the UBV file.
#[derive(Debug, Clone)]
pub struct RawRecord {
    /// Absolute byte offset of this record in the file.
    pub file_offset: u64,
    /// Track ID from bytes 1-2 of the tag.
    pub track_id: u16,
    /// Decoded format code from bytes 4-5.
    pub format_code: FormatCode,
    /// Sequence counter from bytes 6-7.
    pub sequence: u16,
    /// Decoding timestamp (DTS), 32 or 64 bit depending on format.
    pub dts: u64,
    /// Clock rate in Hz (from table lookup or stream).
    pub clock_rate: u32,
    /// Extra field value when format_code.has_extra() (bit 1).
    pub extra: Option<u32>,
    /// Duration field when bit 6 is clear (separate from payload size).
    pub duration: Option<u32>,
    /// Payload data size in bytes.
    pub data_size: u32,
    /// Absolute byte offset of the payload data in the file.
    pub data_offset: u64,
    /// Total record size on disk (header + SIZE field + data + pad + back_size).
    pub total_size: u64,
    /// Raw payload bytes for small records (data_size <= 1024). None for large payloads.
    pub payload: Option<Vec<u8>>,
}

/// Read the next record from the stream. Returns None at EOF.
pub fn read_record<R: Read + Seek>(reader: &mut R) -> Result<Option<RawRecord>> {
    let file_offset = reader.stream_position().map_err(UbvError::Io)?;

    // Read bytes 0-7 (tag + format code + sequence)
    let mut header = [0u8; 8];
    match reader.read_exact(&mut header) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(UbvError::Io(e)),
    }

    // Validate magic byte. A zero byte means we've hit zero-padded trailing
    // space at the end of the file — treat as EOF.
    if header[0] == 0x00 {
        return Ok(None);
    }
    if header[0] != 0xA0 {
        return Err(UbvError::BadMagic {
            offset: file_offset,
            got: header[0],
        });
    }

    // Verify XOR checksum: byte0 ^ byte1 ^ byte2 == byte3
    let expected_checksum = header[0] ^ header[1] ^ header[2];
    if expected_checksum != header[3] {
        return Err(UbvError::ChecksumMismatch {
            offset: file_offset,
            expected: expected_checksum,
            got: header[3],
        });
    }

    let track_id = u16::from_be_bytes([header[1], header[2]]);
    let format_code = FormatCode::new(header[4], header[5]);
    let sequence = u16::from_be_bytes([header[6], header[7]]);

    let header_len = format_code.header_len();

    // Read remaining header bytes (beyond the initial 8) into a stack buffer.
    // Max extra bytes: 4 (clock_rate) + 8 (DTS 64-bit) + 4 (extra) + 4 (duration) = 20.
    let extra_header_bytes = header_len - 8;
    let mut ext_header_buf = [0u8; 24];
    let ext_header = &mut ext_header_buf[..extra_header_bytes];
    reader
        .read_exact(ext_header)
        .map_err(|_| UbvError::UnexpectedEof {
            offset: file_offset,
        })?;

    // Parse fields from extended header based on format code flags
    let mut pos = 0;

    let truncated = || UbvError::UnexpectedEof { offset: file_offset };

    // Clock rate: from stream if sri==1, else from table
    let clock_rate = if format_code.sample_rate_index() == 1 {
        let cr = read_u32_from_slice(ext_header, pos).ok_or_else(truncated)?;
        pos += 4;
        cr
    } else {
        format_code.table_clock_rate()
    };

    // DTS
    let dts = if format_code.dts_64bit() {
        let v = read_u64_from_slice(ext_header, pos).ok_or_else(truncated)?;
        pos += 8;
        v
    } else {
        let v = read_u32_from_slice(ext_header, pos).ok_or_else(truncated)? as u64;
        pos += 4;
        v
    };

    // Extra field (bit 1)
    let extra = if format_code.has_extra() {
        let v = read_u32_from_slice(ext_header, pos).ok_or_else(truncated)?;
        pos += 4;
        Some(v)
    } else {
        None
    };

    // Duration field: when bit 6 is clear, there's a separate duration before SIZE
    let duration = if format_code.byte4() & 0x40 == 0 {
        Some(read_u32_from_slice(ext_header, pos).ok_or_else(truncated)?)
    } else {
        None
    };

    // Read the SIZE field (4 bytes right after header)
    let mut size_buf = [0u8; 4];
    reader
        .read_exact(&mut size_buf)
        .map_err(|_| UbvError::UnexpectedEof {
            offset: file_offset,
        })?;
    let data_size = u32::from_be_bytes(size_buf);

    let data_offset = file_offset + header_len as u64 + 4;

    let pad = alignment_padding(file_offset, header_len, data_size);

    // Capture payload for small records (enables partition header, clock sync, etc.)
    let payload = if data_size <= MAX_INLINE_PAYLOAD {
        let mut payload_buf = vec![0u8; data_size as usize];
        reader.read_exact(&mut payload_buf).map_err(|_| {
            UbvError::UnexpectedEof {
                offset: file_offset,
            }
        })?;
        // Still need to seek past padding and back_size
        // Note: the "extra padding" from bit 0 is stored internally in the packet
        // struct but is NOT written to disk. Only alignment padding appears on disk.
        // Seek past pad + back_size
        reader
            .seek(SeekFrom::Current(pad as i64 + 4))
            .map_err(UbvError::Io)?;
        Some(payload_buf)
    } else {
        // Seek past DATA + PAD + BACK_SIZE
        let skip = data_size as i64 + pad as i64 + 4; // +4 for BACK_SIZE
        reader
            .seek(SeekFrom::Current(skip))
            .map_err(UbvError::Io)?;
        None
    };

    let back_size_value = header_len as u32 + 4 + data_size + pad;
    let total_size = back_size_value as u64 + 4; // +4 for the BACK_SIZE field itself

    Ok(Some(RawRecord {
        file_offset,
        track_id,
        format_code,
        sequence,
        dts,
        clock_rate,
        extra,
        duration,
        data_size,
        data_offset,
        total_size,
        payload,
    }))
}

/// Compute alignment padding to the next 4-byte boundary.
/// `record_prefix_len` is `file_offset + header_len + 4 + data_size`.
fn alignment_padding(file_offset: u64, header_len: usize, data_size: u32) -> u32 {
    let unpadded = file_offset + header_len as u64 + 4 + data_size as u64;
    ((4 - (unpadded % 4)) % 4) as u32
}

fn read_u32_from_slice(buf: &[u8], offset: usize) -> Option<u32> {
    buf.get(offset..offset + 4)?
        .try_into()
        .ok()
        .map(u32::from_be_bytes)
}

fn read_u64_from_slice(buf: &[u8], offset: usize) -> Option<u64> {
    buf.get(offset..offset + 8)?
        .try_into()
        .ok()
        .map(u64::from_be_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_parse_partition_header_old() {
        // Old file partition header at offset 0x00
        let data: Vec<u8> = vec![
            // Tag + format + seq (bytes 0-7)
            0xa0, 0x00, 0x09, 0xa9, 0xfd, 0x0c, 0x00, 0x00,
            // DTS 64-bit (bytes 8-15)
            0x00, 0x00, 0x00, 0x17, 0xde, 0xc4, 0x98, 0xab,
            // SIZE = 20 (bytes 16-19)
            0x00, 0x00, 0x00, 0x14,
            // DATA (20 bytes)
            0x00, 0x00, 0x00, 0x00, 0x3f, 0xf9, 0xec, 0x70,
            0x64, 0x5d, 0xc6, 0x17, 0x02, 0x68, 0x03, 0x03,
            0xe4, 0x00, 0x28, 0xdd,
            // BACK_SIZE = 40 (no padding needed: (0+16+4+20)%4=0)
            0x00, 0x00, 0x00, 0x28,
        ];
        let mut cursor = Cursor::new(data);
        let rec = read_record(&mut cursor).unwrap().unwrap();
        assert_eq!(rec.track_id, 9);
        assert_eq!(rec.format_code.header_len(), 16);
        assert_eq!(rec.data_size, 20);
        assert_eq!(rec.data_offset, 20); // 0 + 16 + 4
    }

    #[test]
    fn test_parse_clock_sync_old() {
        // Clock sync at offset 0x34 in old file (we simulate at offset 0)
        let data: Vec<u8> = vec![
            // Tag
            0xa0, 0xda, 0x7e, 0x04,
            // Format code F9 02, seq 00 00
            0xf9, 0x02, 0x00, 0x00,
            // DTS 32-bit
            0x43, 0xe5, 0xbd, 0x6e,
            // SIZE = 8
            0x00, 0x00, 0x00, 0x08,
            // DATA (8 bytes): seconds + nanoseconds
            0x64, 0x5d, 0xc6, 0x12, 0x34, 0xed, 0xce, 0x00,
            // BACK_SIZE = 24 (12+4+8+0=24)
            0x00, 0x00, 0x00, 0x18,
        ];
        let mut cursor = Cursor::new(data);
        let rec = read_record(&mut cursor).unwrap().unwrap();
        assert_eq!(rec.track_id, 0xDA7E);
        assert_eq!(rec.dts, 0x43E5BD6E);
        assert_eq!(rec.clock_rate, 1000);
        assert_eq!(rec.data_size, 8);
        assert!(rec.payload.is_some());
        let payload = rec.payload.unwrap();
        assert_eq!(payload.len(), 8);
        // Verify seconds and nanoseconds
        let seconds = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let nanos = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
        assert_eq!(seconds, 0x645DC612); // 1683867154
        assert_eq!(nanos, 0x34EDCE00); // 888000000
    }

}
