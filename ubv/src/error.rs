use thiserror::Error;

#[derive(Error, Debug)]
pub enum UbvError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("I/O error at record offset 0x{offset:X} ({context}): {source}")]
    IoAtOffset {
        offset: u64,
        context: &'static str,
        source: std::io::Error,
    },

    #[error("bad magic byte at offset 0x{offset:X}: expected 0xA0, got 0x{got:02X}")]
    BadMagic { offset: u64, got: u8 },

    #[error("checksum mismatch at offset 0x{offset:X}: expected 0x{expected:02X}, got 0x{got:02X}")]
    ChecksumMismatch { offset: u64, expected: u8, got: u8 },

    #[error("unexpected EOF at offset 0x{offset:X}")]
    UnexpectedEof { offset: u64 },

    #[error("back-size mismatch at offset 0x{offset:X}: expected {expected}, got {got}")]
    BackSizeMismatch { offset: u64, expected: u32, got: u32 },

    #[error("clock sync payload too short at record offset 0x{offset:X}: expected at least {expected} bytes, got {got}")]
    ShortPayload {
        offset: u64,
        expected: usize,
        got: usize,
    },
}

pub type Result<T> = std::result::Result<T, UbvError>;
