use std::io;
use std::sync::Once;

extern crate ffmpeg_next as ffmpeg;
use ffmpeg::{codec, encoder, format, Rational};

use crate::analysis::{generate_timecode, AnalysedPartition, AnalysedTrack};
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
    nominal_fps: Rational,
    hevc: bool,
) -> io::Result<()> {
    let ist = ictx.stream(0).unwrap();
    let mut ost = octx
        .add_stream(encoder::find(codec::Id::None))
        .map_err(ffmpeg_err)?;
    ost.set_parameters(ist.parameters());
    ost.set_rate(nominal_fps);
    ost.set_avg_frame_rate(nominal_fps);

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

/// Write video packets with VFR timestamps from DTS values.
///
/// Each packet gets DTS/PTS from the analysed track's rebased dts_values,
/// with duration computed from the delta to the next frame.
fn write_video_packets_vfr(
    ictx: &mut format::context::Input,
    octx: &mut format::context::Output,
    dts_values: &[u64],
    clock_rate: u32,
    out_stream_index: usize,
) -> io::Result<()> {
    let ost_time_base = octx.stream(out_stream_index).unwrap().time_base();
    let input_tb = Rational(1, clock_rate as i32);
    let mut frame_index: usize = 0;

    for (stream, mut packet) in ictx.packets() {
        if stream.index() != 0 {
            continue;
        }

        if frame_index >= dts_values.len() {
            log::warn!(
                "FFmpeg produced more video packets ({}) than DTS values ({}), dropping excess",
                frame_index + 1,
                dts_values.len()
            );
            break;
        }

        let dts = dts_values[frame_index] as i64;
        let duration = if frame_index + 1 < dts_values.len() {
            (dts_values[frame_index + 1] as i64 - dts).max(1)
        } else if frame_index > 0 {
            // Last frame: repeat previous delta
            (dts - dts_values[frame_index - 1] as i64).max(1)
        } else {
            1
        };

        packet.set_pts(Some(dts));
        packet.set_dts(Some(dts));
        packet.set_duration(duration);
        packet.rescale_ts(input_tb, ost_time_base);
        packet.set_position(-1);
        packet.set_stream(out_stream_index);
        packet.write_interleaved(octx).map_err(ffmpeg_err)?;
        frame_index += 1;
    }

    if frame_index < dts_values.len() {
        log::warn!(
            "FFmpeg produced fewer video packets ({}) than DTS values ({})",
            frame_index,
            dts_values.len()
        );
    }

    Ok(())
}

/// Write video packets with CFR timestamps (legacy --force-rate behaviour).
fn write_video_packets_cfr(
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

/// Write audio packets with VFR timestamps from DTS values.
fn write_audio_packets_vfr(
    ictx: &mut format::context::Input,
    octx: &mut format::context::Output,
    dts_values: &[u64],
    clock_rate: u32,
    out_stream_index: usize,
) -> io::Result<()> {
    let ost_time_base = octx.stream(out_stream_index).unwrap().time_base();
    let input_tb = Rational(1, clock_rate as i32);
    let mut frame_index: usize = 0;

    for (stream, mut packet) in ictx.packets() {
        if stream.index() != 0 {
            continue;
        }

        if frame_index >= dts_values.len() {
            log::warn!(
                "FFmpeg produced more audio packets ({}) than DTS values ({}), dropping excess",
                frame_index + 1,
                dts_values.len()
            );
            break;
        }

        let dts = dts_values[frame_index] as i64;
        let duration = if frame_index + 1 < dts_values.len() {
            (dts_values[frame_index + 1] as i64 - dts).max(1)
        } else if frame_index > 0 {
            (dts - dts_values[frame_index - 1] as i64).max(1)
        } else {
            1
        };

        packet.set_pts(Some(dts));
        packet.set_dts(Some(dts));
        packet.set_duration(duration);
        packet.rescale_ts(input_tb, ost_time_base);
        packet.set_position(-1);
        packet.set_stream(out_stream_index);
        packet.write_interleaved(octx).map_err(ffmpeg_err)?;
        frame_index += 1;
    }

    if frame_index < dts_values.len() {
        log::warn!(
            "FFmpeg produced fewer audio packets ({}) than DTS values ({})",
            frame_index,
            dts_values.len()
        );
    }

    Ok(())
}

/// Write audio packets copying timestamps from FFmpeg's demuxer (legacy CFR path).
fn write_audio_packets_cfr(
    audio_ictx: &mut format::context::Input,
    octx: &mut format::context::Output,
    out_stream_index: usize,
) -> io::Result<()> {
    let audio_ist_time_base = audio_ictx
        .stream(0)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "No stream in audio input file")
        })?
        .time_base();
    let audio_ost_time_base = octx.stream(out_stream_index).unwrap().time_base();

    for (stream, mut packet) in audio_ictx.packets() {
        if stream.index() != 0 {
            continue;
        }
        packet.rescale_ts(audio_ist_time_base, audio_ost_time_base);
        packet.set_position(-1);
        packet.set_stream(out_stream_index);
        packet.write_interleaved(octx).map_err(ffmpeg_err)?;
    }
    Ok(())
}

/// Write the output header, optionally with faststart movflag.
fn write_header(octx: &mut format::context::Output, fast_start: bool) -> io::Result<()> {
    if fast_start {
        let mut opts = ffmpeg::Dictionary::new();
        opts.set("movflags", "faststart");
        octx.write_header_with(opts).map_err(ffmpeg_err)?;
    } else {
        octx.write_header().map_err(ffmpeg_err)?;
    }
    Ok(())
}

/// Mux video and/or audio into MP4 using FFmpeg via ffmpeg-next.
pub fn mux(
    partition: &AnalysedPartition,
    video_file: Option<&str>,
    video_track_num: u16,
    audio_file: Option<&str>,
    mp4_file: &str,
    force_rate: Option<u32>,
    fast_start: bool,
) -> io::Result<()> {
    ensure_init();

    match (video_file, audio_file) {
        (Some(vf), Some(af)) => {
            mux_audio_and_video(partition, vf, video_track_num, af, mp4_file, force_rate, fast_start)
        }
        (Some(vf), None) => mux_video_only(partition, vf, video_track_num, mp4_file, force_rate, fast_start),
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
    fast_start: bool,
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
    let cfr = force_rate.is_some();
    let nominal_fps = force_rate.unwrap_or(video_track.nominal_fps);
    let rate = Rational(nominal_fps as i32, 1);

    let mut ictx = format::input(&video_file).map_err(ffmpeg_err)?;
    let mut octx = format::output(&mp4_file).map_err(ffmpeg_err)?;

    add_video_output_stream(&ictx, &mut octx, rate, hevc)?;
    set_timecode_metadata(&mut octx, video_track, rate);

    write_header(&mut octx, fast_start)?;

    if cfr {
        log::info!("Video: CFR {} fps (forced)", nominal_fps);
        write_video_packets_cfr(&mut ictx, &mut octx, rate, 0)?;
    } else {
        log::info!("Video: VFR nominal {} fps, {} Hz timebase", nominal_fps, video_track.clock_rate);
        write_video_packets_vfr(
            &mut ictx,
            &mut octx,
            &video_track.dts_values,
            video_track.clock_rate,
            0,
        )?;
    }

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
    fast_start: bool,
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
            return mux_video_only(partition, video_file, video_track_num, mp4_file, force_rate, fast_start);
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
    let cfr = force_rate.is_some();
    let nominal_fps = force_rate.unwrap_or(video_track.nominal_fps);
    let rate = Rational(nominal_fps as i32, 1);

    // Open inputs
    let mut video_ictx = format::input(&video_file).map_err(ffmpeg_err)?;
    let mut audio_ictx = format::input(&audio_file).map_err(ffmpeg_err)?;

    let mut octx = format::output(&mp4_file).map_err(ffmpeg_err)?;

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

    write_header(&mut octx, fast_start)?;

    // Write video
    if cfr {
        log::info!("Video: CFR {} fps (forced)", nominal_fps);
        write_video_packets_cfr(&mut video_ictx, &mut octx, rate, 0)?;
    } else {
        log::info!("Video: VFR {} fps, {} Hz timebase", nominal_fps, video_track.clock_rate);
        write_video_packets_vfr(
            &mut video_ictx,
            &mut octx,
            &video_track.dts_values,
            video_track.clock_rate,
            0,
        )?;
    }

    // Write audio
    if cfr {
        write_audio_packets_cfr(&mut audio_ictx, &mut octx, 1)?;
    } else {
        log::info!("Audio: VFR {} Hz timebase, {} frames", audio_track.clock_rate, audio_track.frame_count);
        write_audio_packets_vfr(
            &mut audio_ictx,
            &mut octx,
            &audio_track.dts_values,
            audio_track.clock_rate,
            1,
        )?;
    }

    octx.write_trailer().map_err(ffmpeg_err)?;
    Ok(())
}
