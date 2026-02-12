use std::io;
use std::sync::Once;

extern crate ffmpeg_next as ffmpeg;
use ffmpeg::{codec, encoder, format, Rational};

use crate::analysis::{generate_timecode, AnalysedPartition, AnalysedTrack, MAX_ACCEPTED_FPS};
use ubv::track::{track_info, TrackType};

static FFMPEG_INIT: Once = Once::new();

fn ensure_init() {
    FFMPEG_INIT.call_once(|| {
        ffmpeg::init().expect("Failed to initialise FFmpeg");
    });
}

fn ffmpeg_err(e: ffmpeg::Error) -> io::Error {
    io::Error::new(io::ErrorKind::Other, format!("FFmpeg error: {}", e))
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

/// Add a video output stream, copying codec parameters from the input and
/// setting the framerate and codec_tag.
fn add_video_output_stream(
    ictx: &format::context::Input,
    octx: &mut format::context::Output,
    rate: Rational,
    hevc: bool,
) -> io::Result<()> {
    let ist = ictx.stream(0).unwrap();
    let mut ost = octx
        .add_stream(encoder::find(codec::Id::None))
        .map_err(ffmpeg_err)?;
    ost.set_parameters(ist.parameters());
    ost.set_rate(rate);
    ost.set_avg_frame_rate(rate);

    if hevc {
        set_codec_tag(&mut ost, u32::from_le_bytes(*b"hvc1"));
    } else {
        set_codec_tag(&mut ost, 0);
    }
    Ok(())
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

/// Write video packets from the input to the output with sequential timestamps.
///
/// Assigns DTS=PTS sequentially (assumes no B-frames, which holds for
/// Ubiquiti Protect cameras using IP-only GOP structures).
fn write_video_packets(
    ictx: &mut format::context::Input,
    octx: &mut format::context::Output,
    rate: Rational,
    out_stream_index: usize,
) -> io::Result<()> {
    let ost_time_base = octx.stream(out_stream_index).unwrap().time_base();
    let video_tb = Rational(rate.denominator(), rate.numerator());
    let mut frame_index: i64 = 0;
    for (stream, mut packet) in ictx.packets() {
        if stream.index() != 0 {
            continue;
        }
        packet.set_pts(Some(frame_index));
        packet.set_dts(Some(frame_index));
        packet.set_duration(1);
        packet.rescale_ts(video_tb, ost_time_base);
        packet.set_position(-1);
        packet.set_stream(out_stream_index);
        packet.write_interleaved(octx).map_err(ffmpeg_err)?;
        frame_index += 1;
    }
    Ok(())
}

/// Try to detect the video framerate from FFmpeg's parsing of the input bitstream.
/// FFmpeg's H.264/HEVC parser extracts VUI timing_info from SPS, giving us the
/// actual framerate encoded in the bitstream rather than an estimate from
/// wall-clock deltas in the UBV container.
fn detect_rate(ictx: &format::context::Input) -> Option<Rational> {
    let stream = ictx.stream(0)?;

    // Try r_frame_rate first (real base framerate from parser)
    let rate = stream.rate();
    if rate.numerator() > 0 && rate.denominator() > 0 {
        return Some(rate);
    }

    // Fall back to avg_frame_rate
    let avg = stream.avg_frame_rate();
    if avg.numerator() > 0 && avg.denominator() > 0 {
        return Some(avg);
    }

    None
}

/// Determine the video framerate to use, with priority:
/// 1. User-specified --force-rate
/// 2. FFmpeg-detected rate from bitstream SPS/VUI
/// 3. Analysis estimate from UBV wall-clock deltas
fn resolve_rate(
    force_rate: Option<u32>,
    ictx: &format::context::Input,
    analysis_rate: u32,
    mp4_file: &str,
) -> Rational {
    if let Some(fr) = force_rate {
        log::info!("Using forced framerate: {} fps for {}", fr, mp4_file);
        return Rational(fr as i32, 1);
    }

    if let Some(detected) = detect_rate(ictx) {
        let fps = detected.numerator() as f64 / detected.denominator() as f64;
        if fps > 0.0 && fps < MAX_ACCEPTED_FPS as f64 {
            let fps_rounded = fps.round() as u32;
            if analysis_rate > 0 && fps_rounded != analysis_rate {
                log::info!(
                    "Video framerate: {}/{} ({:.2} fps) from bitstream (UBV estimate was {} fps). Use --force-rate if incorrect.",
                    detected.numerator(),
                    detected.denominator(),
                    fps,
                    analysis_rate
                );
            } else {
                log::info!(
                    "Video framerate: {}/{} ({:.2} fps)",
                    detected.numerator(),
                    detected.denominator(),
                    fps
                );
            }
            return detected;
        }
    }

    if analysis_rate > 0 {
        log::warn!(
            "Could not detect framerate from bitstream for {}, using UBV estimate: {} fps. Use --force-rate if incorrect.",
            mp4_file,
            analysis_rate
        );
        Rational(analysis_rate as i32, 1)
    } else {
        log::warn!(
            "No framerate detected for {}, defaulting to 1 fps",
            mp4_file
        );
        Rational(1, 1)
    }
}

/// Mux video and/or audio into MP4 using FFmpeg via ffmpeg-next.
pub fn mux(
    partition: &AnalysedPartition,
    video_file: Option<&str>,
    video_track_num: u16,
    audio_file: Option<&str>,
    mp4_file: &str,
    force_rate: Option<u32>,
) -> io::Result<()> {
    ensure_init();

    match (video_file, audio_file) {
        (Some(vf), Some(af)) => {
            mux_audio_and_video(partition, vf, video_track_num, af, mp4_file, force_rate)
        }
        (Some(vf), None) => mux_video_only(partition, vf, video_track_num, mp4_file, force_rate),
        (None, Some(_)) => {
            log::warn!(
                "Audio-only MP4 muxing is not supported without a video track. \
                 Keeping raw audio file for {}",
                mp4_file
            );
            Ok(())
        }
        (None, None) => {
            log::warn!("No audio or video files to mux for {}", mp4_file);
            Ok(())
        }
    }
}

fn mux_video_only(
    partition: &AnalysedPartition,
    video_file: &str,
    video_track_num: u16,
    mp4_file: &str,
    force_rate: Option<u32>,
) -> io::Result<()> {
    let video_track = match &partition.video_track {
        Some(t) => t,
        None => {
            log::warn!("No video track found, skipping {}", mp4_file);
            return Ok(());
        }
    };

    if video_track.frame_count == 0 {
        log::warn!("Video stream contained zero frames! Skipping {}", mp4_file);
        return Ok(());
    }

    let hevc = is_hevc(video_track_num);

    let mut ictx = format::input(&video_file).map_err(ffmpeg_err)?;
    let rate = resolve_rate(force_rate, &ictx, video_track.rate, mp4_file);
    let mut octx = format::output(&mp4_file).map_err(ffmpeg_err)?;

    add_video_output_stream(&ictx, &mut octx, rate, hevc)?;
    set_timecode_metadata(&mut octx, video_track, rate);

    octx.write_header().map_err(ffmpeg_err)?;
    write_video_packets(&mut ictx, &mut octx, rate, 0)?;

    octx.write_trailer().map_err(ffmpeg_err)?;
    Ok(())
}

fn mux_audio_and_video(
    partition: &AnalysedPartition,
    video_file: &str,
    video_track_num: u16,
    audio_file: &str,
    mp4_file: &str,
    force_rate: Option<u32>,
) -> io::Result<()> {
    let video_track = match &partition.video_track {
        Some(t) => t,
        None => {
            log::warn!(
                "Audio-only MP4 muxing is not supported. Keeping raw audio file for {}",
                mp4_file
            );
            return Ok(());
        }
    };

    let audio_track = match &partition.audio_track {
        Some(t) => t,
        None => {
            return mux_video_only(partition, video_file, video_track_num, mp4_file, force_rate);
        }
    };

    if video_track.frame_count == 0 || audio_track.frame_count == 0 {
        log::warn!(
            "Audio/Video stream contained zero frames! Skipping {}",
            mp4_file
        );
        return Ok(());
    }

    let hevc = is_hevc(video_track_num);

    // Open inputs
    let mut video_ictx = format::input(&video_file).map_err(ffmpeg_err)?;
    let rate = resolve_rate(force_rate, &video_ictx, video_track.rate, mp4_file);
    let mut audio_ictx = format::input(&audio_file).map_err(ffmpeg_err)?;

    let mut octx = format::output(&mp4_file).map_err(ffmpeg_err)?;

    // Capture audio input time base (video timestamps are assigned manually)
    let audio_ist_time_base = audio_ictx
        .stream(0)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "No stream in audio input file")
        })?
        .time_base();

    // Add video output stream (index 0)
    add_video_output_stream(&video_ictx, &mut octx, rate, hevc)?;

    // Add audio output stream (index 1)
    {
        let ist = audio_ictx.stream(0).unwrap();
        let mut ost = octx
            .add_stream(encoder::find(codec::Id::None))
            .map_err(ffmpeg_err)?;
        ost.set_parameters(ist.parameters());
        set_codec_tag(&mut ost, 0);
    }

    set_timecode_metadata(&mut octx, video_track, rate);

    // Compute A/V sync offset
    let audio_delay_sec =
        (video_track.start_nanos - audio_track.start_nanos) as f64 / 1_000_000_000.0;
    log::info!("A/V sync offset: {:.3} s", audio_delay_sec);

    octx.write_header().map_err(ffmpeg_err)?;

    // Read audio output time base after write_header (FFmpeg may adjust it)
    let audio_ost_time_base = octx.stream(1).unwrap().time_base();

    write_video_packets(&mut video_ictx, &mut octx, rate, 0)?;

    // Convert audio delay from seconds to output time_base units
    let audio_delay_ts = if audio_ost_time_base.numerator() != 0 {
        (audio_delay_sec * audio_ost_time_base.denominator() as f64
            / audio_ost_time_base.numerator() as f64) as i64
    } else {
        0
    };

    // Write all audio packets with sync offset
    for (stream, mut packet) in audio_ictx.packets() {
        if stream.index() != 0 {
            continue;
        }
        packet.rescale_ts(audio_ist_time_base, audio_ost_time_base);

        // Apply A/V sync offset
        if let Some(pts) = packet.pts() {
            let adjusted = pts + audio_delay_ts;
            if adjusted < 0 {
                continue;
            }
            packet.set_pts(Some(adjusted));
        }
        if let Some(dts) = packet.dts() {
            let adjusted = dts + audio_delay_ts;
            if adjusted < 0 {
                continue;
            }
            packet.set_dts(Some(adjusted));
        }

        packet.set_position(-1);
        packet.set_stream(1);
        packet.write_interleaved(&mut octx).map_err(ffmpeg_err)?;
    }

    octx.write_trailer().map_err(ffmpeg_err)?;
    Ok(())
}
