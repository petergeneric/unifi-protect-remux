/// Codec category classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecCategory {
    Video,
    Audio,
}

/// Information about a codec identified by its tag string.
#[derive(Debug, Clone, Copy)]
pub struct CodecInfo {
    /// Tag string from the UBV format (e.g. "VH264", "AAAC").
    pub tag: &'static str,
    /// Human-readable codec name (e.g. "h264", "aac").
    pub codec_name: &'static str,
    /// Category of the codec.
    pub category: CodecCategory,
}

// Video codecs
pub const VH264: CodecInfo = CodecInfo { tag: "VH264", codec_name: "h264", category: CodecCategory::Video };
pub const VH265: CodecInfo = CodecInfo { tag: "VH265", codec_name: "hevc", category: CodecCategory::Video };
pub const VAV1: CodecInfo = CodecInfo { tag: "VAV1", codec_name: "av1", category: CodecCategory::Video };

// Audio codecs
pub const AAAC: CodecInfo = CodecInfo { tag: "AAAC", codec_name: "aac", category: CodecCategory::Audio };
pub const AOPUS: CodecInfo = CodecInfo { tag: "AOPUS", codec_name: "opus", category: CodecCategory::Audio };
pub const AG711A: CodecInfo = CodecInfo { tag: "AG711A", codec_name: "pcm_alaw", category: CodecCategory::Audio };
