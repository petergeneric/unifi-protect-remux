use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};

use crate::analysis::AnalysedPartition;
use ubv::frame::RecordHeader;
use ubv::track::{TrackType, is_audio_track, track_info};

/// 4-byte Annex B NAL start code.
const NAL_START_CODE: [u8; 4] = [0, 0, 0, 1];

/// True for video tracks whose UBV payload is already an OBU/bitstream the
/// FFmpeg raw demuxer accepts unchanged. Currently only AV1 Low Overhead
/// Bitstream Format.
/// H.264 and HEVC need 32bit length prefixes rewritten to Annex B start codes.
fn is_obu_video(track_id: u16) -> bool {
    matches!(
        track_info(track_id).map(|ti| ti.track_type),
        Some(TrackType::VideoAv1)
    )
}

/// Demux a single partition's frames into raw video and/or audio bitstream files.
///
/// Reads frame data from the .ubv file at the offsets specified in the partition's
/// frame list, writing NAL-separated video and raw audio to the output files.
pub fn demux_partition(
    ubv_path: &str,
    partition: &AnalysedPartition,
    video_path: Option<&str>,
    video_track_num: u16,
    audio_path: Option<&str>,
) -> io::Result<()> {
    // Open .ubv without buffering — we seek heavily
    let mut ubv_file = File::open(ubv_path)?;

    // Open output files with buffered writers
    let mut video_writer = match video_path {
        Some(path) if partition.video_track_count > 0 => Some(BufWriter::new(File::create(path)?)),
        _ => None,
    };

    let mut audio_writer = match audio_path {
        Some(path) if partition.audio_track_count > 0 => Some(BufWriter::new(File::create(path)?)),
        _ => None,
    };

    // Allocate a reusable buffer sized to the largest frame
    let max_size = partition
        .frames
        .iter()
        .map(|f| f.data_size as usize)
        .max()
        .unwrap_or(0);
    let mut buffer = vec![0u8; max_size];

    let obu_video = is_obu_video(video_track_num);

    for frame in &partition.frames {
        if frame.track_id == video_track_num {
            if let Some(ref mut vw) = video_writer {
                if obu_video {
                    write_video_frame_raw(&mut ubv_file, frame, vw, &mut buffer)?;
                } else {
                    write_video_frame(&mut ubv_file, frame, vw, &mut buffer)?;
                }
            }
        } else if is_audio_track(frame.track_id) {
            if let Some(ref mut aw) = audio_writer {
                write_audio_frame(&mut ubv_file, frame, aw, &mut buffer)?;
            }
        }
    }

    // Flush buffered writers
    if let Some(ref mut vw) = video_writer {
        vw.flush()?;
    }
    if let Some(ref mut aw) = audio_writer {
        aw.flush()?;
    }

    Ok(())
}

/// Wrap an IO error with frame-level context (track, file offset, size).
fn frame_io_err(frame: &RecordHeader, context: &str, cause: io::Error) -> io::Error {
    io::Error::new(
        cause.kind(),
        format!(
            "{} (track={}, offset=0x{:X}, size={}): {}",
            context, frame.track_id, frame.data_offset, frame.data_size, cause
        ),
    )
}

/// Iterate over length-prefixed NAL units in a video frame, calling `f` for each NAL payload.
fn for_each_nal<F>(
    ubv_file: &mut File,
    frame: &RecordHeader,
    read_buf: &mut [u8],
    mut f: F,
) -> io::Result<()>
where
    F: FnMut(&[u8]) -> io::Result<()>,
{
    let mut pos = 0u32;
    let frame_size = frame.data_size;

    ubv_file
        .seek(SeekFrom::Start(frame.data_offset))
        .map_err(|e| frame_io_err(frame, "Seek to video frame failed", e))?;

    while pos < frame_size {
        // Read 4-byte NAL length prefix
        let mut len_buf = [0u8; 4];
        ubv_file
            .read_exact(&mut len_buf)
            .map_err(|e| frame_io_err(frame, "Reading NAL length prefix failed", e))?;
        let nal_size = u32::from_be_bytes(len_buf);
        pos += 4;

        if pos + nal_size > frame_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "NAL unit extends beyond frame boundary (track={}, offset=0x{:X}, \
                     frame_size={}, nal_offset={}, nal_size={})",
                    frame.track_id, frame.data_offset, frame_size, pos, nal_size
                ),
            ));
        }

        ubv_file
            .read_exact(&mut read_buf[..nal_size as usize])
            .map_err(|e| frame_io_err(frame, "Reading NAL payload failed", e))?;
        pos += nal_size;

        f(&read_buf[..nal_size as usize])?;
    }

    Ok(())
}

/// Write a video frame: read length-prefixed NALs, each preceded by an Annex B start code.
fn write_video_frame(
    ubv_file: &mut File,
    frame: &RecordHeader,
    writer: &mut impl Write,
    buffer: &mut [u8],
) -> io::Result<()> {
    for_each_nal(ubv_file, frame, buffer, |nal| {
        writer.write_all(&NAL_START_CODE)?;
        writer.write_all(nal)
    })
}

/// Write an AV1 video frame: byte-copy the on-disk payload. UBV stores AV1 in
/// Low Overhead Bitstream Format (each OBU has its own header byte and
/// LEB128 size), which FFmpeg's `obu` raw demuxer accepts directly. (FFmpeg's
/// `av1` demuxer is for Annex B, which is the length-delimited variant.)
fn write_video_frame_raw(
    ubv_file: &mut File,
    frame: &RecordHeader,
    writer: &mut impl Write,
    buffer: &mut [u8],
) -> io::Result<()> {
    let size = frame.data_size as usize;
    ubv_file
        .seek(SeekFrom::Start(frame.data_offset))
        .map_err(|e| frame_io_err(frame, "Seek to video frame failed", e))?;
    ubv_file
        .read_exact(&mut buffer[..size])
        .map_err(|e| frame_io_err(frame, "Reading video frame failed", e))?;
    writer.write_all(&buffer[..size])?;
    Ok(())
}

/// Write an audio frame: raw data copy, no NAL processing.
fn write_audio_frame(
    ubv_file: &mut File,
    frame: &RecordHeader,
    writer: &mut impl Write,
    buffer: &mut [u8],
) -> io::Result<()> {
    ubv_file
        .seek(SeekFrom::Start(frame.data_offset))
        .map_err(|e| frame_io_err(frame, "Seek to audio frame failed", e))?;
    ubv_file
        .read_exact(&mut buffer[..frame.data_size as usize])
        .map_err(|e| frame_io_err(frame, "Reading audio frame failed", e))?;
    writer.write_all(&buffer[..frame.data_size as usize])?;
    Ok(())
}

/// Demux video frames from a UBV file into an FFmpeg-readable bitstream.
///
/// For H.264/HEVC the output is Annex B (each NAL prefixed with `00 00 00 01`).
/// For AV1 the on-disk OBU bitstream is copied through unchanged. The `frames`
/// slice should contain only video frames for `track_id`.
pub fn demux_video_frames(
    ubv_path: &str,
    frames: &[RecordHeader],
    track_id: u16,
    writer: &mut impl Write,
) -> io::Result<()> {
    if frames.is_empty() {
        return Ok(());
    }

    let mut ubv_file = File::open(ubv_path)?;

    let max_size = frames
        .iter()
        .map(|f| f.data_size as usize)
        .max()
        .unwrap_or(0);
    let mut buffer = vec![0u8; max_size];

    let obu_video = is_obu_video(track_id);
    for frame in frames {
        if obu_video {
            write_video_frame_raw(&mut ubv_file, frame, writer, &mut buffer)?;
        } else {
            write_video_frame(&mut ubv_file, frame, writer, &mut buffer)?;
        }
    }

    Ok(())
}

/// Demux audio frames from a UBV file as raw data written to the given writer.
///
/// The `frames` slice should contain only audio frames.
pub fn demux_audio_frames(
    ubv_path: &str,
    frames: &[RecordHeader],
    writer: &mut impl Write,
) -> io::Result<()> {
    if frames.is_empty() {
        return Ok(());
    }

    let mut ubv_file = File::open(ubv_path)?;

    let max_size = frames
        .iter()
        .map(|f| f.data_size as usize)
        .max()
        .unwrap_or(0);
    let mut buffer = vec![0u8; max_size];

    for frame in frames {
        write_audio_frame(&mut ubv_file, frame, writer, &mut buffer)?;
    }

    Ok(())
}

/// Read a single video frame from the UBV file into a packet-ready buffer.
///
/// For H.264/HEVC, UBV's length-prefixed NAL units are converted to Annex B by
/// replacing each 4-byte length prefix with a start code (00 00 00 01); the
/// MOV muxer converts that back to length-prefixed sample data.
///
/// For AV1, the on-disk bytes (Low Overhead Bitstream Format) are copied
/// through unchanged — they are already the OBU stream the MOV muxer expects.
///
/// `read_buf` is a scratch buffer used only on the Annex-B path.
pub fn read_video_frame(
    ubv_file: &mut File,
    frame: &RecordHeader,
    track_id: u16,
    out_buf: &mut Vec<u8>,
    read_buf: &mut [u8],
) -> io::Result<()> {
    if is_obu_video(track_id) {
        out_buf.resize(frame.data_size as usize, 0);
        ubv_file
            .seek(SeekFrom::Start(frame.data_offset))
            .map_err(|e| frame_io_err(frame, "Seek to video frame failed", e))?;
        ubv_file
            .read_exact(out_buf)
            .map_err(|e| frame_io_err(frame, "Reading video frame failed", e))?;
        Ok(())
    } else {
        out_buf.clear();
        for_each_nal(ubv_file, frame, read_buf, |nal| {
            out_buf.extend_from_slice(&NAL_START_CODE);
            out_buf.extend_from_slice(nal);
            Ok(())
        })
    }
}

/// Read a single audio frame from the UBV file into the provided buffer.
///
/// The buffer is resized to fit the frame data.
pub fn read_audio_frame_raw(
    ubv_file: &mut File,
    frame: &RecordHeader,
    buffer: &mut Vec<u8>,
) -> io::Result<()> {
    buffer.resize(frame.data_size as usize, 0);
    ubv_file
        .seek(SeekFrom::Start(frame.data_offset))
        .map_err(|e| frame_io_err(frame, "Seek to audio frame failed", e))?;
    ubv_file
        .read_exact(buffer)
        .map_err(|e| frame_io_err(frame, "Reading audio frame failed", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{}_{}_{}.bin", name, std::process::id(), nanos))
    }

    #[test]
    fn demux_video_frames_writes_annexb_without_trailing_start_code() {
        // Two length-prefixed NAL units: [AA BB] and [CC]
        let frame_data: Vec<u8> = vec![
            0, 0, 0, 2, 0xAA, 0xBB, // NAL 1
            0, 0, 0, 1, 0xCC, // NAL 2
        ];

        let ubv_path = temp_file_path("demux_annexb_test");
        fs::write(&ubv_path, &frame_data).unwrap();

        let frame = RecordHeader {
            track_id: ubv::track::TRACK_VIDEO,
            data_offset: 0,
            data_size: frame_data.len() as u32,
            dts: 0,
            clock_rate: 90_000,
            sequence: 0,
            keyframe: true,
        };

        let mut output = Vec::new();
        demux_video_frames(
            ubv_path.to_str().unwrap(),
            &[frame],
            ubv::track::TRACK_VIDEO,
            &mut output,
        )
        .unwrap();

        let expected = vec![
            0, 0, 0, 1, 0xAA, 0xBB, // NAL 1
            0, 0, 0, 1, 0xCC, // NAL 2
        ];
        assert_eq!(output, expected);

        let _ = fs::remove_file(ubv_path);
    }

    #[test]
    fn demux_video_frames_passes_av1_obu_bytes_through_unchanged() {
        // Two synthetic AV1 frames in Low Overhead Bitstream Format:
        //   Frame 1: TD (12 00) + Sequence Header (0a 02 aa bb) + Frame OBU (32 02 cc dd)
        //   Frame 2: TD (12 00) + Frame OBU (32 02 ee ff)
        let frame_a: Vec<u8> = vec![0x12, 0x00, 0x0a, 0x02, 0xAA, 0xBB, 0x32, 0x02, 0xCC, 0xDD];
        let frame_b: Vec<u8> = vec![0x12, 0x00, 0x32, 0x02, 0xEE, 0xFF];

        let mut file_data = Vec::new();
        file_data.extend_from_slice(&frame_a);
        file_data.extend_from_slice(&frame_b);

        let ubv_path = temp_file_path("demux_av1_test");
        fs::write(&ubv_path, &file_data).unwrap();

        let frames = [
            RecordHeader {
                track_id: ubv::track::TRACK_VIDEO_AV1,
                data_offset: 0,
                data_size: frame_a.len() as u32,
                dts: 0,
                clock_rate: 90_000,
                sequence: 0,
                keyframe: true,
            },
            RecordHeader {
                track_id: ubv::track::TRACK_VIDEO_AV1,
                data_offset: frame_a.len() as u64,
                data_size: frame_b.len() as u32,
                dts: 4_500,
                clock_rate: 90_000,
                sequence: 1,
                keyframe: false,
            },
        ];

        let mut output = Vec::new();
        demux_video_frames(
            ubv_path.to_str().unwrap(),
            &frames,
            ubv::track::TRACK_VIDEO_AV1,
            &mut output,
        )
        .unwrap();

        // AV1 path is a verbatim byte copy — no NAL re-framing.
        assert_eq!(output, file_data);

        // read_video_frame on the second frame should also yield raw OBU bytes.
        let mut ubv_file = std::fs::File::open(&ubv_path).unwrap();
        let mut out_buf = Vec::new();
        let mut scratch = vec![0u8; frame_b.len()];
        read_video_frame(
            &mut ubv_file,
            &frames[1],
            ubv::track::TRACK_VIDEO_AV1,
            &mut out_buf,
            &mut scratch,
        )
        .unwrap();
        assert_eq!(out_buf, frame_b);

        let _ = fs::remove_file(ubv_path);
    }
}
