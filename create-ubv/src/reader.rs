//! Read an MP4, emit:
//!   * Video frames in UBV's native wire format. For H.264/HEVC: length-prefixed
//!     NAL units with SPS/PPS (or VPS/SPS/PPS) injected inline on every
//!     keyframe. For AV1: Low Overhead Bitstream Format OBUs prefixed by a
//!     Temporal Delimiter on every frame plus the Sequence Header OBU on
//!     keyframes (extracted from the av1C extradata, since MP4 samples must
//!     not carry it inline).
//!   * AAC audio frames wrapped in ADTS headers (real UBV files store audio
//!     this way, and the remux probe path uses the "aac" demuxer which
//!     expects ADTS).

extern crate ffmpeg_next as ffmpeg;

use std::io;
use std::path::Path;

use ffmpeg::codec::Id as CodecId;
use ffmpeg::media::Type;

/// Which in-band video codec this file carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    H264,
    Hevc,
    Av1,
}

/// One video frame ready to be wrapped in a UBV record.
#[derive(Debug)]
pub struct VideoFrame {
    pub dts_90k: u64,
    pub keyframe: bool,
    /// Frame payload in UBV's on-the-wire format for this codec. For H.264/HEVC,
    /// length-prefixed NAL units with SPS/PPS (or VPS/SPS/PPS) prepended on
    /// keyframes. For AV1, Low Overhead Bitstream Format OBUs with a Temporal
    /// Delimiter prepended to every frame and a Sequence Header prepended on
    /// keyframes.
    pub wire_payload: Vec<u8>,
}

pub struct VideoStream {
    pub codec: Codec,
    pub frames: Vec<VideoFrame>,
}

/// One AAC audio frame, ADTS-wrapped.
#[derive(Debug)]
pub struct AudioFrame {
    /// DTS in sample units (clock = sample_rate).
    pub dts_samples: u64,
    /// Full ADTS frame (7-byte header + raw AAC payload).
    pub adts_frame: Vec<u8>,
}

pub struct AudioStream {
    pub sample_rate: u32,
    /// UBV sample-rate index (byte 5 low nibble) for this stream's sample rate.
    pub sri: u8,
    pub frames: Vec<AudioFrame>,
}

pub struct StreamBundle {
    pub video: VideoStream,
    pub audio: Option<AudioStream>,
}

/// Open an MP4 file and extract video (always) and audio (if present, AAC only).
pub fn read_streams(path: &Path) -> io::Result<StreamBundle> {
    ffmpeg::init().map_err(|e| io::Error::other(format!("ffmpeg init failed: {e}")))?;

    let mut ictx = ffmpeg::format::input(&path)
        .map_err(|e| io::Error::other(format!("open {:?}: {e}", path)))?;

    let video_stream = ictx
        .streams()
        .best(Type::Video)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no video stream in MP4"))?;
    let video_index = video_stream.index();
    let video_plan = plan_video(&video_stream)?;

    let audio_plan_opt = ictx
        .streams()
        .best(Type::Audio)
        .map(|s| plan_audio(&s).map(|p| (s.index(), p)))
        .transpose()?;

    // First pass: collect raw packets per stream.
    let mut raw_video: Vec<(i64, bool, Vec<u8>)> = Vec::new();
    let mut raw_audio: Vec<(i64, Vec<u8>)> = Vec::new();
    for (s, packet) in ictx.packets() {
        let si = s.index();
        let data = match packet.data() {
            Some(d) => d.to_vec(),
            None => continue,
        };
        let dts = packet.dts().unwrap_or(0);
        if si == video_index {
            raw_video.push((dts, packet.is_key(), data));
        } else if let Some((ai, _)) = &audio_plan_opt
            && si == *ai
        {
            raw_audio.push((dts, data));
        }
    }

    let video = build_video(video_plan, raw_video)?;
    let audio = match audio_plan_opt {
        Some((_, plan)) if !raw_audio.is_empty() => Some(build_audio(plan, raw_audio)?),
        _ => None,
    };

    Ok(StreamBundle { video, audio })
}

struct VideoPlan {
    codec: Codec,
    tb_num: i64,
    tb_den: i64,
    /// Bytes prepended to every keyframe payload before the demuxed packet.
    keyframe_prefix: Vec<u8>,
    /// Bytes prepended to every non-keyframe payload before the demuxed packet.
    non_keyframe_prefix: Vec<u8>,
}

/// AV1 Temporal Delimiter OBU with `obu_has_size_field=1` and zero-length
/// payload. Inserted at the start of every AV1 frame so a single-frame
/// extraction is a complete temporal unit.
const AV1_TD_OBU: [u8; 2] = [0x12, 0x00];

fn plan_video(stream: &ffmpeg::format::stream::Stream) -> io::Result<VideoPlan> {
    let codec = match stream.parameters().id() {
        CodecId::H264 => Codec::H264,
        CodecId::HEVC => Codec::Hevc,
        CodecId::AV1 => Codec::Av1,
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported video codec {:?}; need H.264, HEVC or AV1",
                    other
                ),
            ));
        }
    };

    // Timebase: convert stream ticks -> 90 kHz via
    //   dts_90k = dts_stream * 90000 * tb.num / tb.den
    let tb = stream.time_base();
    let tb_num = tb.numerator() as i64;
    let tb_den = tb.denominator() as i64;
    if tb_num <= 0 || tb_den <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid video stream timebase {tb_num}/{tb_den}"),
        ));
    }

    let params = stream.parameters();
    let extradata = codec_extradata(&params);
    let (keyframe_prefix, non_keyframe_prefix) = match codec {
        Codec::H264 => (extract_h264_param_sets(&extradata)?, Vec::new()),
        Codec::Hevc => (extract_hevc_param_sets(&extradata)?, Vec::new()),
        Codec::Av1 => {
            // Both prefixes start with a TD OBU; keyframes additionally inline
            // the Sequence Header(s) from av1C so downstream probing can
            // discover codec parameters without seeing the MP4's av1C box.
            let seq_hdr = extract_av1_sequence_header(&extradata)?;
            let mut kf = Vec::with_capacity(AV1_TD_OBU.len() + seq_hdr.len());
            kf.extend_from_slice(&AV1_TD_OBU);
            kf.extend_from_slice(&seq_hdr);
            (kf, AV1_TD_OBU.to_vec())
        }
    };

    // MP4 stores H.264/HEVC NALs with a length prefix; only 4-byte prefixes
    // match UBV's wire format directly so we reject anything else. AV1 has no
    // such prefix so the check is skipped.
    if matches!(codec, Codec::H264 | Codec::Hevc) {
        let length_size = match codec {
            Codec::H264 => length_size_h264(&extradata),
            Codec::Hevc => length_size_hevc(&extradata),
            Codec::Av1 => unreachable!(),
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
    }

    Ok(VideoPlan {
        codec,
        tb_num,
        tb_den,
        keyframe_prefix,
        non_keyframe_prefix,
    })
}

fn build_video(plan: VideoPlan, raw: Vec<(i64, bool, Vec<u8>)>) -> io::Result<VideoStream> {
    if raw.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no video packets in MP4",
        ));
    }
    let base_dts = raw[0].0;
    let mut frames = Vec::with_capacity(raw.len());
    for (dts, keyframe, data) in raw {
        let delta = (dts - base_dts) as i128;
        let scaled = delta * 90_000i128 * plan.tb_num as i128 / plan.tb_den as i128;
        let dts_90k = if scaled < 0 { 0u64 } else { scaled as u64 };

        let prefix = if keyframe {
            &plan.keyframe_prefix
        } else {
            &plan.non_keyframe_prefix
        };
        let mut wire_payload = Vec::with_capacity(prefix.len() + data.len());
        wire_payload.extend_from_slice(prefix);
        wire_payload.extend_from_slice(&data);

        frames.push(VideoFrame {
            dts_90k,
            keyframe,
            wire_payload,
        });
    }

    Ok(VideoStream {
        codec: plan.codec,
        frames,
    })
}

struct AudioPlan {
    sample_rate: u32,
    sri: u8,
    profile: u8,
    sfi: u8,
    channel_cfg: u8,
}

fn plan_audio(stream: &ffmpeg::format::stream::Stream) -> io::Result<AudioPlan> {
    let params = stream.parameters();
    if params.id() != CodecId::AAC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unsupported audio codec {:?}; need AAC (other codecs would require \
                 a UBV track type beyond AAC)",
                params.id()
            ),
        ));
    }

    // MP4 AAC stores raw frames (no ADTS) plus an AudioSpecificConfig in
    // extradata that tells us profile/SFI/channels needed to build ADTS headers.
    let extradata = codec_extradata(&params);
    let (profile, sfi, channel_cfg) = parse_aac_asc(&extradata)?;

    let sample_rate = unsafe { (*params.as_ptr()).sample_rate as u32 };
    if sample_rate == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "AAC stream has zero sample rate",
        ));
    }
    let sri = sri_for_sample_rate(sample_rate).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("AAC sample rate {sample_rate} Hz has no matching UBV sample-rate-index"),
        )
    })?;

    Ok(AudioPlan {
        sample_rate,
        sri,
        profile,
        sfi,
        channel_cfg,
    })
}

fn build_audio(plan: AudioPlan, raw: Vec<(i64, Vec<u8>)>) -> io::Result<AudioStream> {
    // AAC-LC is always 1024 samples per frame. We ignore the demuxer's DTS
    // (which is stable but in stream ticks) and synthesise monotonic
    // sample-counter DTS starting at 0 — mirrors how real UBV audio records
    // look in practice.
    const AAC_SAMPLES_PER_FRAME: u64 = 1024;
    let mut frames = Vec::with_capacity(raw.len());
    for (i, (_dts, data)) in raw.into_iter().enumerate() {
        let adts_frame = wrap_adts(plan.profile, plan.sfi, plan.channel_cfg, &data);
        frames.push(AudioFrame {
            dts_samples: i as u64 * AAC_SAMPLES_PER_FRAME,
            adts_frame,
        });
    }
    Ok(AudioStream {
        sample_rate: plan.sample_rate,
        sri: plan.sri,
        frames,
    })
}

fn codec_extradata(params: &ffmpeg::codec::Parameters) -> Vec<u8> {
    unsafe {
        let raw = params.as_ptr();
        let ptr = (*raw).extradata;
        let len = (*raw).extradata_size as usize;
        if ptr.is_null() || len == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(ptr as *const u8, len).to_vec()
        }
    }
}

/// Parse the AudioSpecificConfig (first 2 bytes) out of AAC extradata.
/// Returns (profile, sampling_frequency_index, channel_configuration).
/// Profile is ADTS-form (AudioObjectType − 1, so AAC-LC → 1).
fn parse_aac_asc(extradata: &[u8]) -> io::Result<(u8, u8, u8)> {
    if extradata.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "AAC extradata < 2 bytes; no AudioSpecificConfig",
        ));
    }
    let b0 = extradata[0];
    let b1 = extradata[1];
    let object_type = b0 >> 3;
    let sfi = ((b0 & 0x07) << 1) | (b1 >> 7);
    let channel_cfg = (b1 >> 3) & 0x0F;
    if sfi == 15 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "AAC extradata uses explicit 24-bit sampling frequency (unsupported)",
        ));
    }
    if !(1..=4).contains(&object_type) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("AAC AudioObjectType {object_type} not representable in ADTS (need 1..=4)"),
        ));
    }
    let profile = object_type - 1;
    Ok((profile, sfi, channel_cfg))
}

/// Map a sample rate in Hz to the UBV sample-rate-index (low nibble of byte 5).
fn sri_for_sample_rate(rate: u32) -> Option<u8> {
    // CLOCK_RATES indices with matching sample rates (skip reserved/special).
    for (i, r) in ubv::format::CLOCK_RATES.iter().enumerate() {
        if i < 3 || *r == 0 {
            continue;
        }
        if *r == rate {
            return Some(i as u8);
        }
    }
    None
}

/// Wrap a raw AAC access unit in a 7-byte ADTS header (no CRC).
fn wrap_adts(profile: u8, sfi: u8, channel_cfg: u8, aac: &[u8]) -> Vec<u8> {
    let frame_len = 7 + aac.len();
    // VBR: buffer_fullness = 0x7FF (11 bits all 1s).
    const BUFFER_FULLNESS: u32 = 0x7FF;

    let mut out = Vec::with_capacity(frame_len);
    out.push(0xFF);
    // 1111 0001 — sync (4) + MPEG-4 (1 bit =0) + layer (2 bits =0) + protection_absent (1 =1).
    out.push(0xF1);
    out.push(((profile & 0x03) << 6) | ((sfi & 0x0F) << 2) | ((channel_cfg >> 2) & 0x01));
    out.push(((channel_cfg & 0x03) << 6) | (((frame_len >> 11) & 0x03) as u8));
    out.push(((frame_len >> 3) & 0xFF) as u8);
    out.push((((frame_len & 0x07) << 5) as u8) | ((BUFFER_FULLNESS >> 6) & 0x1F) as u8);
    // low 6 bits of buffer_fullness << 2, with num_raw_data_blocks_in_frame = 0
    out.push(((BUFFER_FULLNESS & 0x3F) as u8) << 2);
    out.extend_from_slice(aac);
    out
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

/// Extract the configOBUs (sequence header + any metadata OBUs) from an
/// av1C extradata blob. The first 4 bytes of av1C are a fixed-shape header;
/// everything after is OBUs already in `obu_has_size_field=1` form, ready
/// to be inlined into a UBV keyframe.
fn extract_av1_sequence_header(extradata: &[u8]) -> io::Result<Vec<u8>> {
    if extradata.len() < 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "av1C extradata < 4 bytes; no AV1CodecConfigurationRecord",
        ));
    }
    // First byte is marker(1)=1 + version(7)=1 = 0x81. Tolerate any version
    // value but require the marker bit so we catch obviously-wrong blobs.
    if extradata[0] & 0x80 == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "av1C extradata missing marker bit in first byte",
        ));
    }
    if extradata.len() == 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "av1C extradata has no configOBUs (no Sequence Header to inline)",
        ));
    }
    Ok(extradata[4..].to_vec())
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
