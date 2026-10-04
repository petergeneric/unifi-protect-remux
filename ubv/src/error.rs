use thiserror::Error;

#[derive(Error, Debug)]
pub enum UbvError {
    /// I/O error at a known file offset (mid-record reads, seeks).
    #[error("I/O error at record offset 0x{offset:X} ({context}): {source}")]
    IoAtOffset {
        offset: u64,
        context: &'static str,
        source: std::io::Error,
    },

    /// I/O error where no meaningful offset is available (e.g. `stream_position`
    /// itself failing). Distinct from `IoAtOffset` so we don't surface a bogus
    /// `offset: 0` that would look like a failure at the start of the file.
    #[error("I/O error ({context}): {source}")]
    Io {
        context: &'static str,
        source: std::io::Error,
    },

    #[error("bad record magic byte at offset 0x{offset:X}: expected 0xA0, got 0x{got:02X}")]
    BadRecordMagic { offset: u64, got: u8 },

    /// `track_id` here is the *as-read* value from the same bytes the checksum
    /// was meant to validate, so it could itself be corrupt. Displayed with a
    /// `?` to make that uncertainty explicit.
    #[error(
        "checksum mismatch at offset 0x{offset:X} (track? 0x{track_id:04X}): expected 0x{expected:02X}, got 0x{got:02X}"
    )]
    ChecksumMismatch {
        offset: u64,
        track_id: u16,
        expected: u8,
        got: u8,
    },

    #[error("unexpected EOF at offset 0x{offset:X} ({context})")]
    UnexpectedEof { offset: u64, context: &'static str },

    #[error("back-size mismatch at offset 0x{offset:X}: expected {expected}, got {got}")]
    BackSizeMismatch {
        offset: u64,
        expected: u64,
        got: u32,
    },

    #[error(
        "payload too short at record offset 0x{offset:X} (track 0x{track_id:04X}): expected at least {expected} bytes, got {got}"
    )]
    ShortPayload {
        offset: u64,
        track_id: u16,
        expected: usize,
        got: usize,
    },

    #[error("unsupported record layout at offset 0x{offset:X}: format 0x{format:04X}")]
    UnsupportedLayout { offset: u64, format: u16 },

    #[error("invalid clock rate at record offset 0x{offset:X}: expected a nonzero rate, got {got}")]
    InvalidClockRate { offset: u64, got: u32 },

    #[error("record extent overflow at offset 0x{offset:X}")]
    ExtentOverflow { offset: u64 },

    #[error("nonzero byte at offset 0x{offset:X} after padding started at 0x{padding_start:X}")]
    NonzeroPadding { offset: u64, padding_start: u64 },

    #[error("input length changed during parsing: expected {expected}, got {got}")]
    InputChanged { expected: u64, got: u64 },

    #[error("{source}; record at 0x{offset:X}, track 0x{track_id:04X}, format 0x{format:04X}")]
    RecordContext {
        offset: u64,
        track_id: u16,
        format: u16,
        #[source]
        source: Box<UbvError>,
    },

    /// Wraps another error with parser state captured at the point of failure.
    /// The prior record's offset/track/size lets the user spot misaligned reads
    #[error("{source}; parser state: {state}")]
    ParseContext {
        state: String,
        #[source]
        source: Box<UbvError>,
    },
}

pub type Result<T> = std::result::Result<T, UbvError>;
