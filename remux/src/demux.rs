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

/// Write a video frame: read length-prefixed NALs and emit with 00 00 00 01 separators.
fn write_video_frame(
    ubv_file: &mut File,
    frame: &RecordHeader,
    writer: &mut BufWriter<File>,
    buffer: &mut [u8],
) -> io::Result<()> {
    let mut pos = 0u32;
    let frame_size = frame.data_size;

    // Seek to frame start; subsequent reads are sequential within the frame
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

        // Read NAL payload
        ubv_file.read_exact(&mut buffer[..nal_size as usize])?;
        pos += nal_size;

        // Write NAL payload followed by separator
        writer.write_all(&buffer[..nal_size as usize])?;
        writer.write_all(&NAL_START_CODE)?;
    }

    Ok(())
}

/// Write an audio frame: raw data copy, no NAL processing.
fn write_audio_frame(
    ubv_file: &mut File,
    frame: &RecordHeader,
    writer: &mut BufWriter<File>,
    buffer: &mut [u8],
) -> io::Result<()> {
    ubv_file.seek(SeekFrom::Start(frame.data_offset))?;
    ubv_file.read_exact(&mut buffer[..frame.data_size as usize])?;
    writer.write_all(&buffer[..frame.data_size as usize])?;
    Ok(())
}
