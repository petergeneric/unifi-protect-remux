use crate::error::UbvError;

/// Complete means that all input bytes are validated envelopes or verified
/// trailing padding. An error status always describes only an inspected prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    Eof,
    ZeroPadding,
    Error,
}

#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct ReadFailure {
    /// Record start for record errors, offending byte for padding errors, or
    /// the accepted-prefix boundary when no more precise position is available.
    pub offset: u64,
    pub track_id: Option<u16>,
    pub format_code: Option<u16>,
    pub expected_back_size: Option<u64>,
    pub observed_back_size: Option<u32>,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct PaddingExtent {
    pub start: u64,
    pub length: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "jsonschema", derive(schemars::JsonSchema))]
pub struct ReadStatus {
    pub end_reason: EndReason,
    /// End of the last accepted record, never the cursor after a failed seek.
    pub validated_end_offset: u64,
    /// Length of the input stream (decompressed for gzip inputs).
    pub file_size: u64,
    pub padding: Option<PaddingExtent>,
    pub failure: Option<ReadFailure>,
}

impl ReadStatus {
    pub fn is_complete(&self) -> bool {
        self.failure.is_none()
            && match self.end_reason {
                EndReason::Eof => {
                    self.padding.is_none() && self.validated_end_offset == self.file_size
                }
                EndReason::ZeroPadding => self.padding.as_ref().is_some_and(|p| {
                    p.start == self.validated_end_offset
                        && p.start.checked_add(p.length) == Some(self.file_size)
                }),
                EndReason::Error => false,
            }
    }
}

pub(crate) fn failure_details(error: &UbvError, fallback: u64) -> ReadFailure {
    let mut failure = ReadFailure {
        offset: fallback,
        track_id: None,
        format_code: None,
        expected_back_size: None,
        observed_back_size: None,
        message: error.to_string(),
    };
    let mut source = error;
    loop {
        match source {
            UbvError::ParseContext { source: inner, .. } => source = inner,
            UbvError::RecordContext {
                offset,
                track_id,
                format,
                source: inner,
            } => {
                failure.offset = *offset;
                failure.track_id = Some(*track_id);
                failure.format_code = Some(*format);
                source = inner;
            }
            UbvError::BackSizeMismatch {
                offset,
                expected,
                got,
            } => {
                failure.offset = *offset;
                failure.expected_back_size = Some(*expected);
                failure.observed_back_size = Some(*got);
                break;
            }
            UbvError::BadRecordMagic { offset, .. }
            | UbvError::UnexpectedEof { offset, .. }
            | UbvError::UnsupportedLayout { offset, .. }
            | UbvError::InvalidClockRate { offset, .. }
            | UbvError::ExtentOverflow { offset }
            | UbvError::NonzeroPadding { offset, .. }
            | UbvError::IoAtOffset { offset, .. } => {
                failure.offset = *offset;
                break;
            }
            UbvError::ChecksumMismatch {
                offset, track_id, ..
            }
            | UbvError::ShortPayload {
                offset, track_id, ..
            } => {
                failure.offset = *offset;
                failure.track_id = Some(*track_id);
                break;
            }
            _ => break,
        }
    }
    failure
}
