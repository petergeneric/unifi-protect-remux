//! Read an MP4, emit video frames in UBV's native wire format:
//! length-prefixed NAL units with SPS/PPS (or VPS/SPS/PPS) injected inline
//! on every keyframe so downstream probing can discover them.

extern crate ffmpeg_next as ffmpeg;

use std::io;
use std::path::Path;

use ffmpeg::codec::Id as CodecId;
use ffmpeg::media::Type;

/// Which in-band codec this file carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    H264,
    Hevc,
}

/// One video frame ready to be wrapped in a UBV record.
#[derive(Debug)]
pub struct VideoFrame {
    pub dts_90k: u64,
    pub keyframe: bool,
    /// NAL units in UBV's wire format: each NAL preceded by a 4-byte big-endian
    /// length prefix. Keyframes have SPS/PPS (or VPS/SPS/PPS) prepended.
    pub length_prefixed_nals: Vec<u8>,
}

pub struct VideoStream {
    pub codec: Codec,
    pub frames: Vec<VideoFrame>,
}

/// Open an MP4 file and return all video frames reformatted for UBV.
pub fn read_video_frames(path: &Path) -> io::Result<VideoStream> {
    ffmpeg::init()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("ffmpeg init failed: {e}")))?;

    let mut ictx = ffmpeg::format::input(&path)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("open {:?}: {e}", path)))?;

    let stream = ictx
        .streams()
        .best(Type::Video)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no video stream in MP4"))?;

    let stream_index = stream.index();
    let codec = match stream.parameters().id() {
        CodecId::H264 => Codec::H264,
        CodecId::HEVC => Codec::Hevc,
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported codec {:?}; need H.264 or HEVC", other),
            ))
        }
    };

    // Timebase conversion: stream packets use stream.time_base (a rational
    // number of seconds per tick). UBV video uses 90 kHz. The scale factor is
    //   dts_90k = dts_stream * 90000 * tb.num / tb.den
    // We compute in u128 to avoid overflow on long files.
    let tb = stream.time_base();
    let tb_num = tb.numerator() as i64;
    let tb_den = tb.denominator() as i64;
    if tb_num <= 0 || tb_den <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid stream timebase {tb_num}/{tb_den}"),
        ));
    }

    // Extract SPS/PPS (or VPS/SPS/PPS) from codecpar.extradata for inline
    // injection on keyframes. The extradata is in avcC/hvcC form.
    let params = stream.parameters();
    let extradata = unsafe {
        let raw = params.as_ptr();
        let ptr = (*raw).extradata;
        let len = (*raw).extradata_size as usize;
        if ptr.is_null() || len == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(ptr as *const u8, len).to_vec()
        }
    };
    let keyframe_prefix = match codec {
        Codec::H264 => extract_h264_param_sets(&extradata)?,
        Codec::Hevc => extract_hevc_param_sets(&extradata)?,
    };

    // For H.264/HEVC in MP4, packets are length-prefixed. The prefix size is
    // encoded in the extradata (lengthSizeMinusOne field). We only support
    // 4-byte prefixes since that is by far the most common in practice and
    // matches UBV's wire format verbatim (no re-encoding needed for packet body).
    let length_size = match codec {
        Codec::H264 => length_size_h264(&extradata),
        Codec::Hevc => length_size_hevc(&extradata),
    };
    if length_size != 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "MP4 uses {}-byte NAL length prefix; only 4-byte supported",
                length_size
            ),
        ));
    }

    // First pass: collect packets with their raw DTS (may be negative-ish if
    // FFmpeg returns NOPTS). We'll rebase to non-negative and rescale to 90 kHz.
    let mut raw_packets: Vec<(i64, bool, Vec<u8>)> = Vec::new();
    for (s, packet) in ictx.packets() {
        if s.index() != stream_index {
            continue;
        }
        let dts = packet.dts().unwrap_or(0);
        let keyframe = packet.is_key();
        let data = packet
            .data()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "empty packet from MP4 demuxer")
            })?
            .to_vec();
        raw_packets.push((dts, keyframe, data));
    }

    if raw_packets.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no video packets in MP4",
        ));
    }

    let base_dts = raw_packets[0].0;
    let mut frames = Vec::with_capacity(raw_packets.len());
    for (dts, keyframe, data) in raw_packets {
        // Rescale (dts - base_dts) * 90000 * tb_num / tb_den.
        let delta = (dts - base_dts) as i128;
        let scaled = delta * 90_000i128 * tb_num as i128 / tb_den as i128;
        let dts_90k = if scaled < 0 { 0u64 } else { scaled as u64 };

        let payload = if keyframe {
            let mut out = Vec::with_capacity(keyframe_prefix.len() + data.len());
            out.extend_from_slice(&keyframe_prefix);
            out.extend_from_slice(&data);
            out
        } else {
            data
        };

        frames.push(VideoFrame {
            dts_90k,
            keyframe,
            length_prefixed_nals: payload,
        });
    }

    Ok(VideoStream { codec, frames })
}

/// Extract SPS and PPS NAL units from an avcC extradata blob and return
/// them as a concatenated length-prefixed byte sequence.
fn extract_h264_param_sets(extradata: &[u8]) -> io::Result<Vec<u8>> {
    if extradata.len() < 7 || extradata[0] != 0x01 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "extradata is not avcC (missing 0x01 version byte)",
        ));
    }
    let mut out = Vec::new();
    let mut pos = 5;

    // numOfSequenceParameterSets (low 5 bits)
    let n_sps = (extradata[pos] & 0x1F) as usize;
    pos += 1;
    for _ in 0..n_sps {
        let len = read_u16_be(extradata, pos)? as usize;
        pos += 2;
        if pos + len > extradata.len() {
            return Err(truncated("avcC SPS"));
        }
        out.extend_from_slice(&(len as u32).to_be_bytes());
        out.extend_from_slice(&extradata[pos..pos + len]);
        pos += len;
    }

    // numOfPictureParameterSets
    if pos >= extradata.len() {
        return Err(truncated("avcC PPS count"));
    }
    let n_pps = extradata[pos] as usize;
    pos += 1;
    for _ in 0..n_pps {
        let len = read_u16_be(extradata, pos)? as usize;
        pos += 2;
        if pos + len > extradata.len() {
            return Err(truncated("avcC PPS"));
        }
        out.extend_from_slice(&(len as u32).to_be_bytes());
        out.extend_from_slice(&extradata[pos..pos + len]);
        pos += len;
    }

    Ok(out)
}

/// Extract VPS/SPS/PPS NAL units from an hvcC extradata blob.
/// hvcC stores parameter sets in arrays indexed by NAL unit type.
fn extract_hevc_param_sets(extradata: &[u8]) -> io::Result<Vec<u8>> {
    if extradata.len() < 23 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "hvcC extradata too short",
        ));
    }
    let num_arrays = extradata[22] as usize;
    let mut pos = 23;
    let mut out = Vec::new();
    for _ in 0..num_arrays {
        if pos + 3 > extradata.len() {
            return Err(truncated("hvcC array header"));
        }
        // pos[0] = array_completeness(1) + reserved(1) + NAL_unit_type(6) — unused here.
        let num_nalus = read_u16_be(extradata, pos + 1)? as usize;
        pos += 3;
        for _ in 0..num_nalus {
            let len = read_u16_be(extradata, pos)? as usize;
            pos += 2;
            if pos + len > extradata.len() {
                return Err(truncated("hvcC NAL"));
            }
            out.extend_from_slice(&(len as u32).to_be_bytes());
            out.extend_from_slice(&extradata[pos..pos + len]);
            pos += len;
        }
    }
    Ok(out)
}

fn length_size_h264(extradata: &[u8]) -> u8 {
    extradata.get(4).map(|b| (b & 0x03) + 1).unwrap_or(4)
}

fn length_size_hevc(extradata: &[u8]) -> u8 {
    extradata.get(21).map(|b| (b & 0x03) + 1).unwrap_or(4)
}

fn read_u16_be(buf: &[u8], pos: usize) -> io::Result<u16> {
    buf.get(pos..pos + 2)
        .ok_or_else(|| truncated("extradata u16"))
        .map(|s| u16::from_be_bytes([s[0], s[1]]))
}

fn truncated(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::UnexpectedEof,
        format!("truncated {what} in extradata"),
    )
}
