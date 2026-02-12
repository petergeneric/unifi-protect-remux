use thiserror::Error;

#[derive(Error, Debug)]
pub enum UbvError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("bad magic byte at offset {offset}: expected 0xA0, got 0x{got:02X}")]
    BadMagic { offset: u64, got: u8 },

    #[error("checksum mismatch at offset {offset}: expected 0x{expected:02X}, got 0x{got:02X}")]
    ChecksumMismatch { offset: u64, expected: u8, got: u8 },

    #[error("unexpected EOF at offset {offset}")]
    UnexpectedEof { offset: u64 },

    #[error("back-size mismatch at offset {offset}: expected {expected}, got {got}")]
    BackSizeMismatch { offset: u64, expected: u32, got: u32 },

    #[error("clock sync payload too short: expected at least {expected} bytes, got {got}")]
    ShortPayload { expected: usize, got: usize },
}

pub type Result<T> = std::result::Result<T, UbvError>;
