use std::fs::File;
use std::io;
use std::sync::Once;

extern crate ffmpeg_next as ffmpeg;
use ffmpeg::{codec, encoder, format, Rational};

use crate::analysis::{generate_timecode, AnalysedPartition, AnalysedTrack};
use ubv::frame::RecordHeader;
use ubv::track::{is_audio_track, track_info, TrackType};

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
        crate::probe::probe_stream_params(ubv_path, &video_frames, video_track_num, true)?;
    let audio_params = match audio_track {
        Some(_) if !audio_frames.is_empty() => Some(crate::probe::probe_stream_params(
            ubv_path,
            &audio_frames,
            audio_frames[0].track_id,
            false,
        )?),
        _ => None,
    };

    // Create MP4 output
    let mut octx = format::output(&mp4_file).map_err(ffmpeg_err)?;

    // Add video stream (index 0)
    {
        let mut ost = octx
            .add_stream(encoder::find(codec::Id::None))
            .map_err(ffmpeg_err)?;
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
    let has_audio = audio_params.is_some();
    if let Some(params) = audio_params {
        let mut ost = octx
            .add_stream(encoder::find(codec::Id::None))
            .map_err(ffmpeg_err)?;
        ost.set_parameters(params);
        set_codec_tag(&mut ost, 0);
    }

    set_timecode_metadata(&mut octx, video_track, rate);
    write_header(&mut octx, fast_start)?;

    // Write video packets
    //
    // Packets are in Annex B format (start code separated NALs) to match the Annex B
    // extradata produced by probing. The MOV muxer detects the Annex B format and
    // converts both extradata and packet data to the length-prefixed format required
    // by the MP4 container (hvcC/avcC boxes and sample data).
    {
        let mut ubv_file = File::open(ubv_path)?;
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
                )?;
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
                packet.write_interleaved(&mut octx).map_err(ffmpeg_err)?;
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
                )?;
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
                packet.write_interleaved(&mut octx).map_err(ffmpeg_err)?;
            }
        }
    }

    // Write audio packets
    if let (true, Some(at)) = (has_audio, audio_track) {
        let mut ubv_file = File::open(ubv_path)?;
        let mut audio_buf = Vec::new();
        let audio_stream_idx = 1;
        let ost_time_base = octx.stream(audio_stream_idx).unwrap().time_base();
        let input_tb = Rational(1, at.clock_rate as i32);
        let dts_values = &at.dts_values;

        log::info!(
            "Audio: {} Hz timebase, {} frames",
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
            crate::demux::read_audio_frame_raw(&mut ubv_file, frame, &mut audio_buf)?;
            let (dts, duration) = compute_dts_duration(dts_values, i);
            let mut packet = ffmpeg::Packet::copy(&audio_buf);
            packet.set_pts(Some(dts));
            packet.set_dts(Some(dts));
            packet.set_duration(duration);
            packet.rescale_ts(input_tb, ost_time_base);
            packet.set_position(-1);
            packet.set_stream(audio_stream_idx);
            packet.write_interleaved(&mut octx).map_err(ffmpeg_err)?;
        }
    }

    octx.write_trailer().map_err(ffmpeg_err)?;
    Ok(())
}
