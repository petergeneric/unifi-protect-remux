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
    /// False for "untimed" records: clock index 0 (e.g. format `F1 00`), no DTS
    /// field, SIZE at bytes 8-11 and data from byte 12. `dts` and `clock_rate`
    /// are 0 for them.
    pub has_dts: bool,
}

/// Map an IO error during record parsing to the appropriate UbvError.
///
/// True unexpected-EOF errors (the most common failure) become `UnexpectedEof`
/// with a file offset and the same context string. All other IO errors are
/// preserved with positional context via `IoAtOffset`.
fn io_at_offset(offset: u64, context: &'static str) -> impl FnOnce(std::io::Error) -> UbvError {
    move |e: std::io::Error| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            UbvError::UnexpectedEof { offset, context }
        } else {
            UbvError::IoAtOffset {
                offset,
                context,
                source: e,
            }
        }
    }
}

/// Read the next record from the stream. Returns None at EOF.
pub fn read_record<R: Read + Seek>(reader: &mut R) -> Result<Option<RawRecord>> {
    let file_offset = reader.stream_position().map_err(|e| UbvError::Io {
        context: "querying stream position",
        source: e,
    })?;

    // Read bytes 0-7 (tag + format code + sequence)
    let mut header = [0u8; 8];
    match reader.read_exact(&mut header) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(io_at_offset(file_offset, "reading record header")(e)),
    }

    // Validate magic byte. A zero byte means we've hit zero-padded trailing
    // space at the end of the file — treat as EOF.
    if header[0] == 0x00 {
        return Ok(None);
    }
    if header[0] != 0xA0 {
        return Err(UbvError::BadRecordMagic {
            offset: file_offset,
            got: header[0],
        });
    }

    let track_id = u16::from_be_bytes([header[1], header[2]]);

    // Verify XOR checksum: byte0 ^ byte1 ^ byte2 == byte3
    let expected_checksum = header[0] ^ header[1] ^ header[2];
    if expected_checksum != header[3] {
        return Err(UbvError::ChecksumMismatch {
            offset: file_offset,
            track_id,
            expected: expected_checksum,
            got: header[3],
        });
    }

    let format_code = FormatCode::new(header[4], header[5]);
    let sequence = u16::from_be_bytes([header[6], header[7]]);

    // Clock index 0: try the untimed layout (no DTS, SIZE at bytes 8-11). It is
    // only accepted when the trailing back-size matches; otherwise fall back to
    // the generic layout below, exactly as before.
    if format_code.sample_rate_index() == 0
        && let Some(rec) = try_read_untimed(reader, file_offset, track_id, format_code, sequence)?
    {
        // The size check alone does not tell the layouts apart when the
        // generic one fits too (DTS == SIZE + 4, roughly): refuse to guess.
        let end = rec.file_offset + rec.total_size;
        let generic_fits = generic_layout_fits(reader, file_offset, format_code)?;
        reader
            .seek(SeekFrom::Start(end))
            .map_err(io_at_offset(file_offset, "seeking past untimed record"))?;
        if generic_fits {
            return Err(UbvError::AmbiguousRecordLayout {
                offset: file_offset,
                track_id,
            });
        }
        return Ok(Some(rec));
    }

    let header_len = format_code.header_len();

    // Read remaining header bytes (beyond the initial 8) into a stack buffer.
    // Max extra bytes: 4 (clock_rate) + 8 (DTS 64-bit) + 4 (extra) + 4 (duration) = 20.
    let extra_header_bytes = header_len - 8;
    let mut ext_header_buf = [0u8; 24];
    let ext_header = &mut ext_header_buf[..extra_header_bytes];
    reader
        .read_exact(ext_header)
        .map_err(io_at_offset(file_offset, "reading extended header"))?;

    // Parse fields from the extended header. `header_len` was computed from the
    // same `format_code` flags, so `ext_header` is exactly sized for the reads
    // below — slice indexing is infallible by construction.
    let mut pos = 0;

    let clock_rate = if format_code.sample_rate_index() == 1 {
        let cr = read_u32(ext_header, pos);
        pos += 4;
        cr
    } else {
        format_code.table_clock_rate()
    };

    let dts = if format_code.dts_64bit() {
        let v = read_u64(ext_header, pos);
        pos += 8;
        v
    } else {
        let v = read_u32(ext_header, pos) as u64;
        pos += 4;
        v
    };

    let extra = if format_code.has_extra() {
        let v = read_u32(ext_header, pos);
        pos += 4;
        Some(v)
    } else {
        None
    };

    // bit 6 clear: duration field precedes SIZE. Otherwise duration doubles as SIZE.
    let duration = if format_code.byte4() & 0x40 == 0 {
        Some(read_u32(ext_header, pos))
    } else {
        None
    };

    // Read the SIZE field (4 bytes right after header)
    let mut size_buf = [0u8; 4];
    reader
        .read_exact(&mut size_buf)
        .map_err(io_at_offset(file_offset, "reading SIZE field"))?;
    let data_size = u32::from_be_bytes(size_buf);

    let data_offset = file_offset + header_len as u64 + 4;

    let pad = alignment_padding(file_offset, header_len, data_size);

    // Capture payload for small records (enables partition header, clock sync, etc.)
    let payload = if data_size <= MAX_INLINE_PAYLOAD {
        let mut payload_buf = vec![0u8; data_size as usize];
        reader
            .read_exact(&mut payload_buf)
            .map_err(io_at_offset(file_offset, "reading record payload"))?;
        // Still need to seek past padding and back_size
        // Note: the "extra padding" from bit 0 is stored internally in the packet
        // struct but is NOT written to disk. Only alignment padding appears on disk.
        // Seek past pad + back_size
        reader
            .seek(SeekFrom::Current(pad as i64 + 4))
            .map_err(io_at_offset(file_offset, "seeking past padding/back-size"))?;
        Some(payload_buf)
    } else {
        // Seek past DATA + PAD + BACK_SIZE
        let skip = data_size as i64 + pad as i64 + 4; // +4 for BACK_SIZE
        reader
            .seek(SeekFrom::Current(skip))
            .map_err(io_at_offset(file_offset, "seeking past large payload"))?;
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
        has_dts: true,
    }))
}

/// Whether the record at `file_offset` also has a matching back-size when read
/// with the generic (timed) layout. Leaves the stream position undefined.
fn generic_layout_fits<R: Read + Seek>(
    reader: &mut R,
    file_offset: u64,
    format_code: FormatCode,
) -> Result<bool> {
    let header_len = format_code.header_len();
    let read_u32_at = |reader: &mut R, at: u64| -> Result<Option<u32>> {
        reader
            .seek(SeekFrom::Start(at))
            .map_err(io_at_offset(file_offset, "probing generic layout"))?;
        let mut b = [0u8; 4];
        match reader.read_exact(&mut b) {
            Ok(()) => Ok(Some(u32::from_be_bytes(b))),
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(io_at_offset(file_offset, "probing generic layout")(e)),
        }
    };
    let Some(size) = read_u32_at(reader, file_offset + header_len as u64)? else {
        return Ok(false);
    };
    let back = header_len as u64
        + 4
        + size as u64
        + alignment_padding(file_offset, header_len, size) as u64;
    Ok(read_u32_at(reader, file_offset + back)?.is_some_and(|v| v as u64 == back))
}

/// Length of the fixed part of an untimed record before its data: bytes 0-7
/// (tag, format code, sequence) plus the SIZE field at bytes 8-11.
const UNTIMED_PREFIX_LEN: u64 = 12;

/// Try to read an untimed record (clock index 0, no DTS) whose first 8 bytes
/// have already been consumed. Layout: SIZE (u32 BE) at bytes 8-11, DATA,
/// alignment padding, BACK_SIZE = 12 + SIZE + pad.
///
/// Returns `None` -- with the stream positioned again right after the first 8
/// bytes -- when the back-size does not match or lies beyond the end of the
/// stream, so the caller can parse the record the generic way.
fn try_read_untimed<R: Read + Seek>(
    reader: &mut R,
    file_offset: u64,
    track_id: u16,
    format_code: FormatCode,
    sequence: u16,
) -> Result<Option<RawRecord>> {
    let after_tag = file_offset + 8;
    let rewind = |reader: &mut R| -> Result<Option<RawRecord>> {
        reader
            .seek(SeekFrom::Start(after_tag))
            .map_err(io_at_offset(
                file_offset,
                "rewinding after untimed-record probe",
            ))?;
        Ok(None)
    };

    let mut size_buf = [0u8; 4];
    match reader.read_exact(&mut size_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return rewind(reader),
        Err(e) => return Err(io_at_offset(file_offset, "reading untimed SIZE field")(e)),
    }
    let data_size = u32::from_be_bytes(size_buf);
    // SIZE sits where the generic layout would put the DTS, i.e. header_len = 8.
    let pad = alignment_padding(file_offset, 8, data_size);
    let back_size_value = UNTIMED_PREFIX_LEN + data_size as u64 + pad as u64;

    // A back-size that does not fit in u32 cannot match the on-disk field.
    let Ok(expected_back) = u32::try_from(back_size_value) else {
        return rewind(reader);
    };

    let data_offset = file_offset + UNTIMED_PREFIX_LEN;
    reader
        .seek(SeekFrom::Start(file_offset + back_size_value))
        .map_err(io_at_offset(file_offset, "seeking to untimed back-size"))?;
    let mut back_buf = [0u8; 4];
    match reader.read_exact(&mut back_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return rewind(reader),
        Err(e) => return Err(io_at_offset(file_offset, "reading untimed back-size")(e)),
    }
    let end = file_offset + back_size_value + 4;
    if u32::from_be_bytes(back_buf) != expected_back {
        return rewind(reader);
    }

    let payload = if data_size <= MAX_INLINE_PAYLOAD {
        let mut payload_buf = vec![0u8; data_size as usize];
        reader
            .seek(SeekFrom::Start(data_offset))
            .map_err(io_at_offset(file_offset, "seeking to untimed payload"))?;
        reader
            .read_exact(&mut payload_buf)
            .map_err(io_at_offset(file_offset, "reading untimed payload"))?;
        reader
            .seek(SeekFrom::Start(end))
            .map_err(io_at_offset(file_offset, "seeking past untimed record"))?;
        Some(payload_buf)
    } else {
        // Already positioned right after BACK_SIZE.
        None
    };

    Ok(Some(RawRecord {
        file_offset,
        track_id,
        format_code,
        sequence,
        dts: 0,
        clock_rate: 0,
        extra: None,
        duration: None,
        data_size,
        data_offset,
        total_size: end - file_offset,
        payload,
        has_dts: false,
    }))
}

/// Compute alignment padding to the next 4-byte boundary.
/// `record_prefix_len` is `file_offset + header_len + 4 + data_size`.
fn alignment_padding(file_offset: u64, header_len: usize, data_size: u32) -> u32 {
    let unpadded = file_offset + header_len as u64 + 4 + data_size as u64;
    ((4 - (unpadded % 4)) % 4) as u32
}

fn read_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(
        buf[offset..offset + 4]
            .try_into()
            .expect("header_len mismatch — parser bug"),
    )
}

fn read_u64(buf: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes(
        buf[offset..offset + 8]
            .try_into()
            .expect("header_len mismatch — parser bug"),
    )
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
            0xa0, 0x00, 0x09, 0xa9, 0xfd, 0x0c, 0x00, 0x00, // DTS 64-bit (bytes 8-15)
            0x00, 0x00, 0x00, 0x17, 0xde, 0xc4, 0x98, 0xab, // SIZE = 20 (bytes 16-19)
            0x00, 0x00, 0x00, 0x14, // DATA (20 bytes)
            0x00, 0x00, 0x00, 0x00, 0x3f, 0xf9, 0xec, 0x70, 0x64, 0x5d, 0xc6, 0x17, 0x02, 0x68,
            0x03, 0x03, 0xe4, 0x00, 0x28, 0xdd,
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
            0xa0, 0xda, 0x7e, 0x04, // Format code F9 02, seq 00 00
            0xf9, 0x02, 0x00, 0x00, // DTS 32-bit
            0x43, 0xe5, 0xbd, 0x6e, // SIZE = 8
            0x00, 0x00, 0x00, 0x08, // DATA (8 bytes): seconds + nanoseconds
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

    use crate::test_util::{UbvBuilder, index_payload};

    #[test]
    fn untimed_record_small_payload() {
        let mut b = UbvBuilder::default();
        let payload = index_payload(&[(1_000, 0x40), (2_000, 0x80)]);
        b.untimed(10, 3, &payload);
        let after = b.video(1, 9_000, true, 5);
        let mut cursor = Cursor::new(b.bytes);

        let rec = read_record(&mut cursor).unwrap().unwrap();
        assert!(!rec.has_dts);
        assert_eq!(rec.track_id, 10);
        assert_eq!(rec.format_code.0, 0xF100);
        assert_eq!(rec.sequence, 3);
        assert_eq!(rec.dts, 0);
        assert_eq!(rec.clock_rate, 0);
        assert_eq!(rec.data_size as usize, payload.len());
        assert_eq!(rec.data_offset, 12);
        assert_eq!(rec.total_size, after);
        assert_eq!(rec.payload.as_deref(), Some(&payload[..]));

        // The stream is left at the next record.
        assert_eq!(cursor.position(), after);
        let next = read_record(&mut cursor).unwrap().unwrap();
        assert!(next.has_dts);
        assert_eq!(next.track_id, 7);
        assert_eq!(next.dts, 9_000);
        assert!(read_record(&mut cursor).unwrap().is_none());
    }

    #[test]
    fn untimed_record_large_payload_with_padding() {
        let mut b = UbvBuilder::default();
        // 2001 bytes: above the inline limit and needs 3 bytes of padding.
        let payload = vec![0u8; 2001];
        b.untimed(6, 0, &payload);
        let len = b.offset();
        let mut cursor = Cursor::new(b.bytes);

        let rec = read_record(&mut cursor).unwrap().unwrap();
        assert!(!rec.has_dts);
        assert_eq!(rec.track_id, 6);
        assert_eq!(rec.data_size, 2001);
        assert!(rec.payload.is_none());
        assert_eq!(rec.total_size, 12 + 2001 + 3 + 4);
        assert_eq!(rec.total_size, len);
        assert_eq!(cursor.position(), len);
        assert!(read_record(&mut cursor).unwrap().is_none());
    }

    /// An `F1 00` record whose back-size does not fit the untimed layout keeps
    /// the generic interpretation (32-bit DTS at bytes 8-11, SIZE at 12-15).
    #[test]
    fn clock_index_zero_without_matching_back_size_keeps_generic_layout() {
        for dts in [0x0000_0010_u32, 0x7FFF_0000] {
            let mut data: Vec<u8> = vec![0xA0, 0x00, 0x0A, 0xAA, 0xF1, 0x00, 0x00, 0x07];
            data.extend_from_slice(&dts.to_be_bytes());
            data.extend_from_slice(&8u32.to_be_bytes());
            data.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
            data.extend_from_slice(&(12u32 + 4 + 8).to_be_bytes());
            let len = data.len() as u64;
            let mut cursor = Cursor::new(data);

            let rec = read_record(&mut cursor).unwrap().unwrap();
            assert!(rec.has_dts);
            assert_eq!(rec.dts, dts as u64);
            assert_eq!(rec.data_size, 8);
            assert_eq!(rec.data_offset, 16);
            assert_eq!(rec.payload.as_deref(), Some(&[1, 2, 3, 4, 5, 6, 7, 8][..]));
            assert_eq!(rec.total_size, len);
            assert_eq!(cursor.position(), len);
        }
    }

    /// A clock-index-0 record that fits both layouts is refused, not guessed.
    #[test]
    fn record_fitting_both_layouts_is_an_error() {
        // Timed reading: DTS = 12, SIZE = 8, back-size 24. Untimed reading:
        // SIZE = 12, back-size 24 too.
        let mut data: Vec<u8> = vec![0xA0, 0x00, 0x0A, 0xAA, 0xF1, 0x00, 0x00, 0x00];
        data.extend_from_slice(&12u32.to_be_bytes());
        data.extend_from_slice(&8u32.to_be_bytes());
        data.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        data.extend_from_slice(&24u32.to_be_bytes());
        let mut cursor = Cursor::new(data);
        let err = read_record(&mut cursor).unwrap_err();
        assert!(
            matches!(
                err,
                UbvError::AmbiguousRecordLayout {
                    offset: 0,
                    track_id: 10
                }
            ),
            "{err:?}"
        );
    }
}
