//! Synthesise a `.ubv` file from a source MP4.
//!
//! Primarily intended to build deterministic test fixtures without needing
//! a real Unifi recording checked in to the repo. The synthesiser preserves
//! only the subset of the UBV format that the rest of the pipeline actually
//! needs: one partition header, one clock-sync record, and a video track
//! carrying inline SPS/PPS (or VPS/SPS/PPS for HEVC) on keyframes.

pub mod reader;
pub mod writer;

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

/// Sample-rate index for the 90 kHz video clock (table entry 12).
const SRI_90K: u8 = 0x0C;
/// Sample-rate index for the 1 kHz millisecond clock (table entry 2).
const SRI_1K: u8 = 0x02;

/// Byte-4 format-code bits. See `ubv::format::FormatCode` for decoding.
mod fc {
    /// Keyframe: bit 7=1 bit 6=1 (Single packet), bit 5=1 (keyframe),
    /// bit 4=0 (no CTS), bit 3=1 (clock rate), bit 2=1 (64-bit DTS),
    /// bit 1=0, bit 0=1.
    pub const KEYFRAME: u8 = 0xFD;
    /// Non-keyframe: same as KEYFRAME but bit 5 cleared.
    pub const NON_KEYFRAME: u8 = 0xDD;
}

/// Configuration for [`synth_from_mp4`].
#[derive(Debug, Clone)]
pub struct SynthConfig {
    /// Wall-clock start time (UTC seconds since the Unix epoch) that will
    /// appear in the output MP4's filename and metadata. Defaults to a fixed
    /// value (2024-01-01T00:00:00Z) for reproducibility.
    pub wall_clock_secs: u32,
}

impl Default for SynthConfig {
    fn default() -> Self {
        // 2024-01-01T00:00:00Z
        Self { wall_clock_secs: 1_704_067_200 }
    }
}

/// Read an MP4 file and write an equivalent-ish `.ubv` file.
pub fn synth_from_mp4(mp4_path: &Path, ubv_path: &Path, config: &SynthConfig) -> io::Result<()> {
    let frames = reader::read_video_frames(mp4_path)?;
    if frames.frames.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "MP4 contains no video frames",
        ));
    }
    if !frames.frames[0].keyframe {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "first MP4 video frame is not a keyframe — cannot anchor the partition",
        ));
    }

    let out = File::create(ubv_path)?;
    let mut out = BufWriter::new(out);
    let mut offset: u64 = 0;

    // 1. Partition header (track 0x0009). Payload is opaque metadata; the
    //    UBV parser only needs the record envelope to exist. We use the same
    //    20-byte blob shape the reader's golden test uses.
    let part_payload: [u8; 20] = [
        0x00, 0x00, 0x00, 0x00, 0x3f, 0xf9, 0xec, 0x70, 0x64, 0x5d, 0xc6, 0x17, 0x02, 0x68, 0x03,
        0x03, 0xe4, 0x00, 0x28, 0xdd,
    ];
    let part = writer::Record {
        track_id: ubv::track::TRACK_PARTITION,
        byte4: fc::KEYFRAME,
        byte5: SRI_90K,
        sequence: 0,
        dts: 0,
        clock_rate_in_stream: None,
        extra: None,
        duration: None,
        payload: &part_payload,
    };
    offset += part.write_to(&mut out, offset)?;

    // 2. Clock sync (track 0xDA7E) anchors wall-clock. Payload is a pair of
    //    u32_be: seconds then nanoseconds since the Unix epoch.
    let mut cs_payload = [0u8; 8];
    cs_payload[0..4].copy_from_slice(&config.wall_clock_secs.to_be_bytes());
    cs_payload[4..8].copy_from_slice(&0u32.to_be_bytes());
    let cs = writer::Record {
        track_id: ubv::track::TRACK_CLOCK_SYNC,
        byte4: fc::KEYFRAME,
        byte5: SRI_1K,
        sequence: 0,
        dts: 0,
        clock_rate_in_stream: None,
        extra: None,
        duration: None,
        payload: &cs_payload,
    };
    offset += cs.write_to(&mut out, offset)?;

    // 3. Video frames (track 7 or 1003). DTS is rescaled to 90 kHz.
    let track_id = match frames.codec {
        reader::Codec::H264 => ubv::track::TRACK_VIDEO,
        reader::Codec::Hevc => ubv::track::TRACK_VIDEO_HEVC,
    };
    for (i, frame) in frames.frames.iter().enumerate() {
        let byte4 = if frame.keyframe {
            fc::KEYFRAME
        } else {
            fc::NON_KEYFRAME
        };
        let rec = writer::Record {
            track_id,
            byte4,
            byte5: SRI_90K,
            sequence: i as u16,
            dts: frame.dts_90k,
            clock_rate_in_stream: None,
            extra: None,
            duration: None,
            payload: &frame.length_prefixed_nals,
        };
        offset += rec.write_to(&mut out, offset)?;
    }

    out.flush()?;
    Ok(())
}
