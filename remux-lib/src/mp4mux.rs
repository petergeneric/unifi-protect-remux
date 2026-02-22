use std::fs::File;
use std::io;
use std::sync::Once;

extern crate ffmpeg_next as ffmpeg;
extern crate ffmpeg_sys_next as ffi;
use ffmpeg::{codec, encoder, format, Rational};

use crate::analysis::{generate_timecode, AnalysedPartition, AnalysedTrack};
use ubv::frame::RecordHeader;
use ubv::track::{is_audio_track, track_info, TrackType};

static FFMPEG_INIT: Once = Once::new();

/// Custom FFmpeg log callback that routes messages through Rust's `log` crate.
///
/// # Safety
/// Called by FFmpeg's internal logging system. Uses `av_log_format_line2` to
/// safely format the variadic arguments into a fixed buffer.
unsafe extern "C" fn ffmpeg_log_callback(
    ptr: *mut libc::c_void,
    level: libc::c_int,
    fmt: *const libc::c_char,
    vl: ffi::va_list,
) {
    // Map FFmpeg log level to Rust log level; ignore messages above our threshold.
    let rust_level = match level {
        ffi::AV_LOG_PANIC | ffi::AV_LOG_FATAL => log::Level::Error,
        ffi::AV_LOG_ERROR => log::Level::Error,
        ffi::AV_LOG_WARNING => log::Level::Warn,
        ffi::AV_LOG_INFO => log::Level::Info,
        ffi::AV_LOG_VERBOSE => log::Level::Debug,
        ffi::AV_LOG_DEBUG | ffi::AV_LOG_TRACE => log::Level::Trace,
        _ => return,
    };

    // Early-out if this level is filtered by the Rust logger.
    if !log::log_enabled!(rust_level) {
        return;
    }

    let mut buf = [0u8; 1024];
    let mut print_prefix: libc::c_int = 1;
    let written = unsafe {
        ffi::av_log_format_line2(
            ptr,
            level,
            fmt,
            vl,
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len() as libc::c_int,
            &mut print_prefix,
        )
    };
    if written < 0 {
        return;
    }

    let len = (written as usize).min(buf.len() - 1);
    // Trim trailing whitespace/newlines that FFmpeg appends.
    let msg = std::str::from_utf8(&buf[..len])
        .unwrap_or_default()
        .trim_end();
    if msg.is_empty() {
        return;
    }

    log::log!(target: "ffmpeg", rust_level, "{}", msg);
}

fn ensure_init() {
    FFMPEG_INIT.call_once(|| {
        ffmpeg::init().expect("Failed to initialise FFmpeg");
        unsafe {
            ffi::av_log_set_callback(Some(ffmpeg_log_callback));
        }
    });
}

fn ffmpeg_err(context: &str) -> impl FnOnce(ffmpeg::Error) -> io::Error + '_ {
    move |e: ffmpeg::Error| {
        io::Error::new(io::ErrorKind::Other, format!("{}: {}", context, e))
    }
}

/// Like `ffmpeg_err` but defers context string construction to the error path,
/// avoiding a `format!` allocation on every successful frame.
fn ffmpeg_err_lazy<F: FnOnce() -> String>(context: F) -> impl FnOnce(ffmpeg::Error) -> io::Error {
    move |e: ffmpeg::Error| {
        io::Error::new(io::ErrorKind::Other, format!("{}: {}", context(), e))
    }
}

fn is_hevc(video_track_num: u16) -> bool {
    track_info(video_track_num)
        .map(|ti| ti.track_type == TrackType::VideoHevc)
        .unwrap_or(false)
}

/// Set the codec_tag on an output stream's codec parameters.
///
/// # Safety
/// Mutates the raw FFmpeg codec parameters pointer.
fn set_codec_tag(ost: &mut ffmpeg::StreamMut, tag: u32) {
    unsafe {
        (*ost.parameters().as_mut_ptr()).codec_tag = tag;
    }
}

/// Maximum DTS value that FFmpeg's MOV muxer supports (signed 32-bit integer).
/// Exceeding this triggers an assertion failure in movenc.c.
const MOV_DTS_MAX: u64 = i32::MAX as u64;

/// Compute a reduced video timescale for the MOV muxer when the default would
/// cause DTS values to exceed the signed 32-bit limit.
///
/// `max_dts_ticks` is the maximum DTS value in the original `clock_rate` units.
/// By default FFmpeg's MOV muxer uses `clock_rate` as the video timescale, so
/// the output DTS equals the input DTS. For recordings longer than ~6.6 hours
/// at 90 kHz, these values overflow.
///
/// Returns `Some(reduced_timescale)` if the default would overflow, `None` otherwise.
fn safe_mov_video_timescale(max_dts_ticks: u64, clock_rate: u32) -> Option<u32> {
    if max_dts_ticks <= MOV_DTS_MAX || clock_rate == 0 {
        return None;
    }
    // With reduced timescale ts, output DTS = max_dts_ticks * ts / clock_rate.
    // We need: max_dts_ticks * ts / clock_rate <= MOV_DTS_MAX
    // => ts <= MOV_DTS_MAX * clock_rate / max_dts_ticks
    let ts = (MOV_DTS_MAX as u128 * clock_rate as u128 / max_dts_ticks as u128) as u32;
    // 5% safety margin for rounding during FFmpeg's internal rescaling
    Some((ts * 95 / 100).clamp(1, clock_rate))
}

/// Set timecode metadata on the output context if a start timecode is available.
fn set_timecode_metadata(
    octx: &mut format::context::Output,
    video_track: &AnalysedTrack,
    rate: Rational,
) {
    if let Some(ref tc) = video_track.start_timecode {
        let fps_int = (rate.numerator() as f64 / rate.denominator() as f64).round() as u32;
        let timecode = generate_timecode(tc, fps_int.max(1));
        let mut metadata = ffmpeg::Dictionary::new();
        metadata.set("timecode", &timecode);
        octx.set_metadata(metadata);
    }
}

/// Write the output header with muxer options (faststart, video_track_timescale).
fn write_header(
    octx: &mut format::context::Output,
    fast_start: bool,
    video_timescale: Option<u32>,
) -> io::Result<()> {
    let has_opts = fast_start || video_timescale.is_some();
    if has_opts {
        let mut opts = ffmpeg::Dictionary::new();
        if fast_start {
            opts.set("movflags", "faststart");
        }
        if let Some(ts) = video_timescale {
            opts.set("video_track_timescale", &ts.to_string());
        }
        octx.write_header_with(opts)
            .map_err(ffmpeg_err("Writing MP4 header"))?;
    } else {
        octx.write_header()
            .map_err(ffmpeg_err("Writing MP4 header"))?;
    }
    Ok(())
}

/// Return the number of audio samples per frame for a given track, used to
/// synthesize monotonic timestamps in CFR mode. AAC-LC is always 1024; Opus
/// at 48 kHz with 20 ms frames is 960. Falls back to 1024 for unknown codecs.
fn audio_samples_per_frame(track_id: u16) -> u32 {
    match track_info(track_id).map(|ti| ti.track_type) {
        Some(TrackType::AudioAac) => 1024,
        Some(TrackType::AudioOpus) => 960,
        // Raw/PCM: frame size varies; 1024 is a safe default for the CFR path
        _ => 1024,
    }
}

/// Compute DTS and duration for a frame from the rebased DTS values array.
fn compute_dts_duration(dts_values: &[u64], index: usize) -> (i64, i64) {
    let dts = dts_values[index] as i64;
    let duration = if index + 1 < dts_values.len() {
        (dts_values[index + 1] as i64 - dts).max(1)
    } else if index > 0 {
        (dts - dts_values[index - 1] as i64).max(1)
    } else {
        1
    };
    (dts, duration)
}

/// Stream UBV frames directly to MP4 without intermediate files.
///
/// Probes codec parameters from the first few frames via AVIO, then reads each
/// frame from the UBV file, wraps it in an FFmpeg Packet, and writes it to the
/// MP4 output. No temporary files are created.
pub fn stream_to_mp4(
    ubv_path: &str,
    partition: &AnalysedPartition,
    video_track_num: u16,
    mp4_file: &str,
    force_rate: Option<u32>,
    fast_start: bool,
) -> io::Result<()> {
    ensure_init();

    let video_track = match &partition.video_track {
        Some(t) if t.frame_count > 0 => t,
        _ => {
            log::warn!("No video track with frames found, skipping {}", mp4_file);
            return Ok(());
        }
    };
    let audio_track = partition
        .audio_track
        .as_ref()
        .filter(|t| t.frame_count > 0);

    let hevc = is_hevc(video_track_num);
    let cfr = force_rate.is_some();
    let nominal_fps = force_rate.unwrap_or(video_track.nominal_fps);
    let rate = Rational(nominal_fps as i32, 1);

    // Separate frames by track
    let video_frames: Vec<RecordHeader> = partition
        .frames
        .iter()
        .filter(|f| f.track_id == video_track_num)
        .copied()
        .collect();
    let audio_frames: Vec<RecordHeader> = partition
        .frames
        .iter()
        .filter(|f| is_audio_track(f.track_id))
        .copied()
        .collect();

    // Probe codec parameters from first few frames
    let video_params =
        crate::probe::probe_stream_params(ubv_path, &video_frames, video_track_num)?;
    let audio_params = match audio_track {
        Some(at) if !audio_frames.is_empty() => Some(crate::probe::probe_stream_params(
            ubv_path,
            &audio_frames,
            at.track_id,
        )?),
        _ => None,
    };

    // Create MP4 output
    let mut octx = format::output(&mp4_file)
        .map_err(ffmpeg_err("Creating MP4 output file"))?;

    // Add video stream (index 0)
    {
        let mut ost = octx
            .add_stream(encoder::find(codec::Id::None))
            .map_err(ffmpeg_err("Adding video stream to MP4"))?;
        ost.set_parameters(video_params);
        ost.set_rate(rate);
        ost.set_avg_frame_rate(rate);
        if hevc {
            set_codec_tag(&mut ost, u32::from_le_bytes(*b"hvc1"));
        } else {
            set_codec_tag(&mut ost, 0);
        }
    }

    // Add audio stream (index 1) if present
    if let Some(params) = audio_params {
        let mut ost = octx
            .add_stream(encoder::find(codec::Id::None))
            .map_err(ffmpeg_err("Adding audio stream to MP4"))?;
        ost.set_parameters(params);
        set_codec_tag(&mut ost, 0);
    }

    // Check for MOV 32-bit DTS overflow on video track.
    // In VFR mode, max DTS comes directly from the rebased DTS values.
    // In CFR mode, the equivalent max DTS in clock_rate units is
    // (num_frames - 1) * clock_rate / nominal_fps.
    let video_max_dts = if cfr {
        if video_frames.len() > 1 && nominal_fps > 0 {
            (video_frames.len() as u64 - 1) * video_track.clock_rate as u64
                / nominal_fps as u64
        } else {
            0
        }
    } else {
        video_track.dts_values.last().copied().unwrap_or(0)
    };
    let video_timescale = safe_mov_video_timescale(video_max_dts, video_track.clock_rate);
    if let Some(ts) = video_timescale {
        log::info!(
            "Video timescale reduced from {} to {} Hz to fit MOV 32-bit DTS limit",
            video_track.clock_rate, ts
        );
    }

    // Warn if audio DTS would also overflow (audio timescale is fixed to the
    // sample rate in MOV, so we cannot reduce it via muxer options).
    if let Some(at) = audio_track {
        let audio_max_dts = if cfr {
            let spf = audio_samples_per_frame(at.track_id) as u64;
            if !audio_frames.is_empty() { (audio_frames.len() as u64 - 1) * spf } else { 0 }
        } else {
            at.dts_values.last().copied().unwrap_or(0)
        };
        if audio_max_dts > MOV_DTS_MAX {
            log::warn!(
                "Audio DTS values exceed MOV 32-bit limit ({} > {}); \
                 output may be corrupt or fail to write",
                audio_max_dts, MOV_DTS_MAX
            );
        }
    }

    set_timecode_metadata(&mut octx, video_track, rate);
    write_header(&mut octx, fast_start, video_timescale)?;

    // Write video packets
    //
    // Packets are in Annex B format (start code separated NALs) to match the Annex B
    // extradata produced by probing. The MOV muxer detects the Annex B format and
    // converts both extradata and packet data to the length-prefixed format required
    // by the MP4 container (hvcC/avcC boxes and sample data).
    let mut ubv_file = File::open(ubv_path)
        .map_err(|e| io::Error::new(e.kind(), format!(
            "Opening UBV file '{}' for frame reading: {}", ubv_path, e
        )))?;
    {
        let max_frame = video_frames
            .iter()
            .map(|f| f.data_size as usize)
            .max()
            .unwrap_or(0);
        let mut read_buf = vec![0u8; max_frame];
        let mut annexb_buf = Vec::with_capacity(max_frame + 1024);
        let ost_time_base = octx.stream(0).unwrap().time_base();

        if cfr {
            log::info!("Video: CFR {} fps (forced)", nominal_fps);
            let video_tb = Rational(rate.denominator(), rate.numerator());
            for (i, frame) in video_frames.iter().enumerate() {
                crate::demux::read_video_frame_annexb(
                    &mut ubv_file,
                    frame,
                    &mut annexb_buf,
                    &mut read_buf,
                )
                .map_err(|e| io::Error::new(e.kind(), format!(
                    "Reading video frame {}/{}: {}", i + 1, video_frames.len(), e
                )))?;
                let mut packet = ffmpeg::Packet::copy(&annexb_buf);
                packet.set_pts(Some(i as i64));
                packet.set_dts(Some(i as i64));
                packet.set_duration(1);
                packet.rescale_ts(video_tb, ost_time_base);
                packet.set_position(-1);
                packet.set_stream(0);
                if frame.keyframe {
                    packet.set_flags(codec::packet::Flags::KEY);
                }
                packet.write_interleaved(&mut octx)
                    .map_err(ffmpeg_err_lazy(|| format!(
                        "Writing video frame {}/{}", i + 1, video_frames.len()
                    )))?;
            }
        } else {
            log::info!(
                "Video: VFR nominal {} fps, {} Hz timebase",
                nominal_fps,
                video_track.clock_rate
            );
            let input_tb = Rational(1, video_track.clock_rate as i32);
            let dts_values = &video_track.dts_values;
            for (i, frame) in video_frames.iter().enumerate() {
                if i >= dts_values.len() {
                    log::warn!(
                        "More video frames ({}) than DTS values ({})",
                        video_frames.len(),
                        dts_values.len()
                    );
                    break;
                }
                crate::demux::read_video_frame_annexb(
                    &mut ubv_file,
                    frame,
                    &mut annexb_buf,
                    &mut read_buf,
                )
                .map_err(|e| io::Error::new(e.kind(), format!(
                    "Reading video frame {}/{}: {}", i + 1, video_frames.len(), e
                )))?;
                let (dts, duration) = compute_dts_duration(dts_values, i);
                let mut packet = ffmpeg::Packet::copy(&annexb_buf);
                packet.set_pts(Some(dts));
                packet.set_dts(Some(dts));
                packet.set_duration(duration);
                packet.rescale_ts(input_tb, ost_time_base);
                packet.set_position(-1);
                packet.set_stream(0);
                if frame.keyframe {
                    packet.set_flags(codec::packet::Flags::KEY);
                }
                packet.write_interleaved(&mut octx)
                    .map_err(ffmpeg_err_lazy(|| format!(
                        "Writing video frame {}/{}", i + 1, video_frames.len()
                    )))?;
            }
        }
    }

    // Write audio packets
    if let Some(at) = audio_track {
        let mut audio_buf = Vec::new();
        let audio_stream_idx = 1;
        let ost_time_base = octx.stream(audio_stream_idx).unwrap().time_base();

        if cfr {
            // CFR path: synthesize monotonic timestamps from the codec's fixed
            // frame duration, matching what FFmpeg's demuxer would produce from a
            // raw bitstream file. For AAC-LC each frame is 1024 samples.
            let samples_per_frame = audio_samples_per_frame(at.track_id);
            let input_tb = Rational(1, at.clock_rate as i32);
            log::info!(
                "Audio: CFR {} Hz, {} samples/frame, {} frames",
                at.clock_rate,
                samples_per_frame,
                at.frame_count
            );
            for (i, frame) in audio_frames.iter().enumerate() {
                crate::demux::read_audio_frame_raw(&mut ubv_file, frame, &mut audio_buf)
                    .map_err(|e| io::Error::new(e.kind(), format!(
                        "Reading audio frame {}/{}: {}", i + 1, audio_frames.len(), e
                    )))?;
                let pts = i as i64 * samples_per_frame as i64;
                let mut packet = ffmpeg::Packet::copy(&audio_buf);
                packet.set_pts(Some(pts));
                packet.set_dts(Some(pts));
                packet.set_duration(samples_per_frame as i64);
                packet.rescale_ts(input_tb, ost_time_base);
                packet.set_position(-1);
                packet.set_stream(audio_stream_idx);
                packet.write_interleaved(&mut octx)
                    .map_err(ffmpeg_err_lazy(|| format!(
                        "Writing audio frame {}/{}", i + 1, audio_frames.len()
                    )))?;
            }
        } else {
            let input_tb = Rational(1, at.clock_rate as i32);
            let dts_values = &at.dts_values;
            log::info!(
                "Audio: VFR {} Hz timebase, {} frames",
                at.clock_rate,
                at.frame_count
            );
            for (i, frame) in audio_frames.iter().enumerate() {
                if i >= dts_values.len() {
                    log::warn!(
                        "More audio frames ({}) than DTS values ({})",
                        audio_frames.len(),
                        dts_values.len()
                    );
                    break;
                }
                crate::demux::read_audio_frame_raw(&mut ubv_file, frame, &mut audio_buf)
                    .map_err(|e| io::Error::new(e.kind(), format!(
                        "Reading audio frame {}/{}: {}", i + 1, audio_frames.len(), e
                    )))?;
                let (dts, duration) = compute_dts_duration(dts_values, i);
                let mut packet = ffmpeg::Packet::copy(&audio_buf);
                packet.set_pts(Some(dts));
                packet.set_dts(Some(dts));
                packet.set_duration(duration);
                packet.rescale_ts(input_tb, ost_time_base);
                packet.set_position(-1);
                packet.set_stream(audio_stream_idx);
                packet.write_interleaved(&mut octx)
                    .map_err(ffmpeg_err_lazy(|| format!(
                        "Writing audio frame {}/{}", i + 1, audio_frames.len()
                    )))?;
            }
        }
    }

    octx.write_trailer().map_err(ffmpeg_err("Writing MP4 trailer"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_timescale_not_needed_when_within_limit() {
        // 1 hour at 90kHz: 3600 * 90000 = 324,000,000 < INT32_MAX
        assert_eq!(safe_mov_video_timescale(324_000_000, 90000), None);
    }

    #[test]
    fn safe_timescale_not_needed_at_boundary() {
        assert_eq!(safe_mov_video_timescale(MOV_DTS_MAX, 90000), None);
    }

    #[test]
    fn safe_timescale_reduces_for_long_recording() {
        // ~25 hours at 90kHz: 90000 * 90000 = 8,100,000,000
        let max_dts: u64 = 8_100_000_000;
        let ts = safe_mov_video_timescale(max_dts, 90000).unwrap();
        // Reduced timescale must produce output DTS within limit
        let output_dts = max_dts as u128 * ts as u128 / 90000;
        assert!(output_dts <= MOV_DTS_MAX as u128,
            "output DTS {} exceeds limit {}", output_dts, MOV_DTS_MAX);
        assert!(ts > 0);
        assert!(ts < 90000);
    }

    #[test]
    fn safe_timescale_handles_zero() {
        assert_eq!(safe_mov_video_timescale(0, 90000), None);
        assert_eq!(safe_mov_video_timescale(1000, 0), None);
    }

    #[test]
    fn safe_timescale_extreme_duration() {
        // 100 hours at 90kHz
        let max_dts: u64 = 100 * 3600 * 90000;
        let ts = safe_mov_video_timescale(max_dts, 90000).unwrap();
        let output_dts = max_dts as u128 * ts as u128 / 90000;
        assert!(output_dts <= MOV_DTS_MAX as u128);
        assert!(ts > 0);
    }
}
