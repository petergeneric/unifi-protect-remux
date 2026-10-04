use std::io::{Read, Seek, SeekFrom};

use crate::error::{Result, UbvError};
use crate::format::{FormatCode, RecordLayout};

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
    /// Decoding timestamp, absent for untimed layouts.
    pub dts: Option<u64>,
    /// Clock rate in Hz (table or stream), absent for untimed layouts.
    pub clock_rate: Option<u32>,
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

/// Map an IO error during record parsing to the appropriate UbvError.
fn io_at_offset(offset: u64, context: &'static str) -> impl FnOnce(std::io::Error) -> UbvError {
    move |source| {
        if source.kind() == std::io::ErrorKind::UnexpectedEof {
            UbvError::UnexpectedEof { offset, context }
        } else {
            UbvError::IoAtOffset {
                offset,
                context,
                source,
            }
        }
    }
}

/// Termination is verified separately from the record envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordEnd {
    Eof,
    ZeroPadding { start: u64, length: u64 },
}

#[derive(Debug)]
pub enum ReadOutcome {
    Record(RawRecord),
    End(RecordEnd),
}

/// Validates envelopes against a captured stream length without loading large
/// payloads. Callers must provide a stable snapshot: length checks detect some,
/// but not all, concurrent writes. Gzip readers use the decompressed length.
pub struct RecordReader<'a, R> {
    reader: &'a mut R,
    file_size: u64,
}

impl<'a, R: Read + Seek> RecordReader<'a, R> {
    pub fn new(reader: &'a mut R) -> Result<Self> {
        let position = reader.stream_position().map_err(|source| UbvError::Io {
            context: "querying stream position",
            source,
        })?;
        let file_size = reader
            .seek(SeekFrom::End(0))
            .map_err(io_at_offset(position, "measuring input"))?;
        reader
            .seek(SeekFrom::Start(position))
            .map_err(io_at_offset(position, "restoring stream position"))?;
        Ok(Self { reader, file_size })
    }

    pub fn file_size(&self) -> u64 {
        self.file_size
    }

    /// Check the captured length before reporting successful termination.
    fn finish(&mut self, end: RecordEnd) -> Result<ReadOutcome> {
        let got = self
            .reader
            .seek(SeekFrom::End(0))
            .map_err(io_at_offset(self.file_size, "checking input length"))?;
        if got != self.file_size {
            return Err(UbvError::InputChanged {
                expected: self.file_size,
                got,
            });
        }
        Ok(ReadOutcome::End(end))
    }

    pub fn read_next(&mut self) -> Result<ReadOutcome> {
        let offset = self
            .reader
            .stream_position()
            .map_err(|source| UbvError::Io {
                context: "querying stream position",
                source,
            })?;
        if offset == self.file_size {
            return self.finish(RecordEnd::Eof);
        }
        if offset > self.file_size {
            return Err(UbvError::UnexpectedEof {
                offset,
                context: "record starts beyond input",
            });
        }
        let mut header = [0u8; 8];
        let available = (self.file_size - offset).min(8) as usize;
        self.reader
            .read_exact(&mut header[..available])
            .map_err(io_at_offset(offset, "reading record header"))?;
        if header[0] == 0 {
            // Scan from the boundary itself, including all bytes already read.
            if let Some(pos) = header[..available].iter().position(|&b| b != 0) {
                return Err(UbvError::NonzeroPadding {
                    offset: offset + pos as u64,
                    padding_start: offset,
                });
            }
            let mut position = offset + available as u64;
            let mut buf = [0u8; 65536];
            while position < self.file_size {
                let n = (self.file_size - position).min(buf.len() as u64) as usize;
                self.reader
                    .read_exact(&mut buf[..n])
                    .map_err(io_at_offset(position, "checking trailing padding"))?;
                if let Some(pos) = buf[..n].iter().position(|&b| b != 0) {
                    return Err(UbvError::NonzeroPadding {
                        offset: position + pos as u64,
                        padding_start: offset,
                    });
                }
                position += n as u64;
            }
            return self.finish(RecordEnd::ZeroPadding {
                start: offset,
                length: self.file_size - offset,
            });
        }
        if available < 8 {
            return Err(UbvError::UnexpectedEof {
                offset,
                context: "reading record header",
            });
        }
        if header[0] != 0xA0 {
            return Err(UbvError::BadRecordMagic {
                offset,
                got: header[0],
            });
        }
        let track_id = u16::from_be_bytes([header[1], header[2]]);
        let expected = header[0] ^ header[1] ^ header[2];
        if expected != header[3] {
            return Err(UbvError::ChecksumMismatch {
                offset,
                track_id,
                expected,
                got: header[3],
            });
        }
        let format_code = FormatCode::new(header[4], header[5]);
        self.read_envelope(
            offset,
            track_id,
            format_code,
            u16::from_be_bytes([header[6], header[7]]),
        )
        .map(ReadOutcome::Record)
        .map_err(|source| UbvError::RecordContext {
            offset,
            track_id,
            format: format_code.0,
            source: Box::new(source),
        })
    }

    fn read_envelope(
        &mut self,
        file_offset: u64,
        track_id: u16,
        format_code: FormatCode,
        sequence: u16,
    ) -> Result<RawRecord> {
        let layout = format_code.layout().ok_or(UbvError::UnsupportedLayout {
            offset: file_offset,
            format: format_code.0,
        })?;
        let header_len = format_code.header_len();
        let mut ext = [0u8; 24];
        let ext = &mut ext[..header_len - 8];
        self.reader
            .read_exact(ext)
            .map_err(io_at_offset(file_offset, "reading extended header"))?;
        let (dts, clock_rate, extra, duration) = match layout {
            RecordLayout::Untimed => (None, None, None, None),
            RecordLayout::Timed => {
                let mut pos = 0;
                let rate = if format_code.sample_rate_index() == 1 {
                    let rate = read_u32(ext, pos);
                    pos += 4;
                    rate
                } else {
                    format_code.table_clock_rate()
                };
                if rate == 0 {
                    return Err(UbvError::InvalidClockRate {
                        offset: file_offset,
                        got: rate,
                    });
                }
                let dts = if format_code.dts_64bit() {
                    let dts = read_u64(ext, pos);
                    pos += 8;
                    dts
                } else {
                    let dts = read_u32(ext, pos) as u64;
                    pos += 4;
                    dts
                };
                let extra = if format_code.has_extra() {
                    let value = read_u32(ext, pos);
                    pos += 4;
                    Some(value)
                } else {
                    None
                };
                let duration = if format_code.byte4() & 0x40 == 0 {
                    Some(read_u32(ext, pos))
                } else {
                    None
                };
                (Some(dts), Some(rate), extra, duration)
            }
        };
        let mut size = [0u8; 4];
        self.reader
            .read_exact(&mut size)
            .map_err(io_at_offset(file_offset, "reading payload size"))?;
        let data_size = u32::from_be_bytes(size);
        let overflow = || UbvError::ExtentOverflow {
            offset: file_offset,
        };
        let data_offset = file_offset
            .checked_add(header_len as u64 + 4)
            .ok_or_else(overflow)?;
        let payload_end = data_offset
            .checked_add(data_size as u64)
            .ok_or_else(overflow)?;
        let pad = (4 - payload_end % 4) % 4;
        let trailer_offset = payload_end.checked_add(pad).ok_or_else(overflow)?;
        let next = trailer_offset.checked_add(4).ok_or_else(overflow)?;
        let back_size = trailer_offset - file_offset;
        if next > self.file_size {
            return Err(UbvError::UnexpectedEof {
                offset: file_offset,
                context: "record extent exceeds input",
            });
        }
        // Small payloads can be read sequentially without discarding buffered
        // input. Large payloads need only a seek and a four-byte trailer read.
        let payload = if data_size <= MAX_INLINE_PAYLOAD {
            let mut buf = vec![0; data_size as usize];
            self.reader
                .read_exact(&mut buf)
                .map_err(io_at_offset(file_offset, "reading payload"))?;
            let mut alignment = [0u8; 3];
            self.reader
                .read_exact(&mut alignment[..pad as usize])
                .map_err(io_at_offset(file_offset, "reading alignment padding"))?;
            Some(buf)
        } else {
            self.reader
                .seek(SeekFrom::Start(trailer_offset))
                .map_err(io_at_offset(file_offset, "seeking to back-size"))?;
            None
        };
        let mut trailer = [0u8; 4];
        self.reader
            .read_exact(&mut trailer)
            .map_err(io_at_offset(file_offset, "reading back-size"))?;
        let got = u32::from_be_bytes(trailer);
        if u64::from(got) != back_size {
            return Err(UbvError::BackSizeMismatch {
                offset: file_offset,
                expected: back_size,
                got,
            });
        }
        Ok(RawRecord {
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
            total_size: next - file_offset,
            payload,
        })
    }
}

/// Compatibility API: only verified EOF or trailing padding becomes `None`.
/// Repeated callers should use `RecordReader` to capture the length once.
pub fn read_record<R: Read + Seek>(reader: &mut R) -> Result<Option<RawRecord>> {
    match RecordReader::new(reader)?.read_next()? {
        ReadOutcome::Record(record) => Ok(Some(record)),
        ReadOutcome::End(_) => Ok(None),
    }
}

fn read_u32(buf: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(
        buf[offset..offset + 4]
            .try_into()
            .expect("header layout mismatch"),
    )
}
fn read_u64(buf: &[u8], offset: usize) -> u64 {
    u64::from_be_bytes(
        buf[offset..offset + 8]
            .try_into()
            .expect("header layout mismatch"),
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
        assert_eq!(rec.dts, Some(0x43E5BD6E));
        assert_eq!(rec.clock_rate, Some(1000));
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
