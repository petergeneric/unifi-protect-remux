use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};

use crate::analysis::AnalysedPartition;
use ubv::frame::RecordHeader;
use ubv::track::is_audio_track;

/// 4-byte Annex B NAL start code.
const NAL_START_CODE: [u8; 4] = [0, 0, 0, 1];

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
        Some(path) if partition.video_track_count > 0 => {
            Some(BufWriter::new(File::create(path)?))
        }
        _ => None,
    };

    let mut audio_writer = match audio_path {
        Some(path) if partition.audio_track_count > 0 => {
            Some(BufWriter::new(File::create(path)?))
        }
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

    // Write opening NAL separator for video
    if let Some(ref mut vw) = video_writer {
        vw.write_all(&NAL_START_CODE)?;
    }

    for frame in &partition.frames {
        if frame.track_id == video_track_num {
            if let Some(ref mut vw) = video_writer {
                write_video_frame(&mut ubv_file, frame, vw, &mut buffer)?;
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

    ubv_file.seek(SeekFrom::Start(frame.data_offset))?;

    while pos < frame_size {
        // Read 4-byte NAL length prefix
        let mut len_buf = [0u8; 4];
        ubv_file.read_exact(&mut len_buf)?;
        let nal_size = u32::from_be_bytes(len_buf);
        pos += 4;

        if pos + nal_size > frame_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "NAL read goes beyond frame: pos={}, nal_size={}, frame_size={}",
                    pos, nal_size, frame_size
                ),
            ));
        }

        ubv_file.read_exact(&mut read_buf[..nal_size as usize])?;
        pos += nal_size;

        f(&read_buf[..nal_size as usize])?;
    }

    Ok(())
}

/// Write a video frame: read length-prefixed NALs and emit with 00 00 00 01 separators.
fn write_video_frame(
    ubv_file: &mut File,
    frame: &RecordHeader,
    writer: &mut impl Write,
    buffer: &mut [u8],
) -> io::Result<()> {
    for_each_nal(ubv_file, frame, buffer, |nal| {
        writer.write_all(nal)?;
        writer.write_all(&NAL_START_CODE)
    })
}

/// Write an audio frame: raw data copy, no NAL processing.
fn write_audio_frame(
    ubv_file: &mut File,
    frame: &RecordHeader,
    writer: &mut impl Write,
    buffer: &mut [u8],
) -> io::Result<()> {
    ubv_file.seek(SeekFrom::Start(frame.data_offset))?;
    ubv_file.read_exact(&mut buffer[..frame.data_size as usize])?;
    writer.write_all(&buffer[..frame.data_size as usize])?;
    Ok(())
}

/// Demux video frames from a UBV file into an Annex B bitstream written to the given writer.
///
/// Writes a leading NAL start code, then each frame's NAL units separated by start codes.
/// The `frames` slice should contain only video frames for the desired track.
pub fn demux_video_frames(
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

    writer.write_all(&NAL_START_CODE)?;
    for frame in frames {
        write_video_frame(&mut ubv_file, frame, writer, &mut buffer)?;
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

/// Read a single video frame from the UBV file into Annex B format.
///
/// Converts UBV's length-prefixed NAL units to Annex B by replacing each 4-byte
/// length prefix with a 4-byte start code (00 00 00 01). Each NAL is preceded by
/// a start code with no trailing start code — suitable for per-frame MP4 packet
/// construction where the MOV muxer converts Annex B to length-prefixed internally.
pub fn read_video_frame_annexb(
    ubv_file: &mut File,
    frame: &RecordHeader,
    annexb_buf: &mut Vec<u8>,
    read_buf: &mut [u8],
) -> io::Result<()> {
    annexb_buf.clear();
    for_each_nal(ubv_file, frame, read_buf, |nal| {
        annexb_buf.extend_from_slice(&NAL_START_CODE);
        annexb_buf.extend_from_slice(nal);
        Ok(())
    })
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
    ubv_file.seek(SeekFrom::Start(frame.data_offset))?;
    ubv_file.read_exact(buffer)?;
    Ok(())
}
