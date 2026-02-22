pub mod analysis;
pub mod demux;
pub mod mp4mux;
pub mod probe;
pub mod thumbnail;

use std::io;
use std::path::Path;

use chrono::SecondsFormat;

use ubv::format::PacketPosition;
use ubv::partition::{Partition, PartitionEntry};
use ubv::track::{is_video_track, track_info};

/// Configuration for remuxing UBV files.
pub struct RemuxConfig {
    pub with_audio: bool,
    pub with_video: bool,
    /// Force a particular video framerate (0 = auto Variable Framerate, otherwise force CFR).
    pub force_rate: u32,
    pub fast_start: bool,
    /// Output directory. `"SRC-FOLDER"` means alongside the source .ubv file.
    pub output_folder: String,
    /// Create MP4 output (false = raw demux).
    pub mp4: bool,
    /// Video track number (7 = H.264, 1003 = HEVC, 1004 = AV1, 0 = auto-detect).
    pub video_track: u16,
    /// Override the base output filename (without extension or timecode).
    /// When `None`, the base name is derived from the input .ubv filename.
    pub base_name: Option<String>,
}

impl Default for RemuxConfig {
    fn default() -> Self {
        Self {
            with_audio: true,
            with_video: true,
            force_rate: 0,
            fast_start: false,
            output_folder: "./".to_string(),
            mp4: true,
            video_track: 0,
            base_name: None,
        }
    }
}

/// Log severity level for progress events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// Events emitted during file processing.
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    Log(LogLevel, String),
    FileStarted { path: String },
    PartitionsFound { count: usize },
    PartitionStarted { index: usize, total: usize },
    OutputGenerated { path: String },
    PartitionError { index: usize, error: String },
    FileCompleted { path: String, outputs: Vec<String>, errors: Vec<String> },
}

/// Result of processing a single .ubv file.
pub struct FileResult {
    pub input_path: String,
    pub output_files: Vec<String>,
    pub errors: Vec<String>,
}

/// Validate a remux configuration (without checking input files).
pub fn validate_config(config: &RemuxConfig) -> Result<(), String> {
    if !config.with_audio && !config.with_video {
        return Err("Must enable extraction of at least one of: audio, video!".into());
    }
    if config.mp4 && !config.with_video {
        return Err(
            "MP4 output requires video; with_video=false is not supported with mp4=true".into(),
        );
    }
    Ok(())
}

/// Find the first video track ID across all partitions.
pub fn detect_video_track(partitions: &[Partition]) -> Option<u16> {
    partitions.iter().find_map(|p| {
        p.entries.iter().find_map(|entry| match entry {
            PartitionEntry::Frame(f) if is_video_track(f.header.track_id) => {
                Some(f.header.track_id)
            }
            _ => None,
        })
    })
}

/// Derive codec name from a track ID via track_info.
pub fn codec_name_for_track(track_id: u16) -> Option<&'static str> {
    track_info(track_id)
        .and_then(|ti| ti.codec)
        .map(|ci| ci.codec_name)
}

fn get_start_timecode(
    partition: &analysis::AnalysedPartition,
    video_track_num: u16,
) -> Option<chrono::DateTime<chrono::Utc>> {
    if partition.video_track_count == 0 {
        partition
            .audio_track
            .as_ref()
            .and_then(|at| at.start_timecode)
    } else {
        partition
            .video_track
            .as_ref()
            .filter(|vt| vt.track_id == video_track_num)
            .and_then(|vt| vt.start_timecode)
    }
}

/// Process a single .ubv file, calling `progress` with events as work proceeds.
///
/// Returns a `FileResult` with the list of output files generated and any
/// per-partition errors that occurred.
pub fn process_file<F>(
    ubv_path: &str,
    config: &RemuxConfig,
    progress: &mut F,
) -> Result<FileResult, Box<dyn std::error::Error>>
where
    F: FnMut(ProgressEvent),
{
    let mut result = FileResult {
        input_path: ubv_path.to_string(),
        output_files: Vec::new(),
        errors: Vec::new(),
    };

    progress(ProgressEvent::FileStarted {
        path: ubv_path.to_string(),
    });
    progress(ProgressEvent::Log(
        LogLevel::Info,
        format!("Analysing {}", ubv_path),
    ));

    if ubv_path.contains("_2_rotating_") || ubv_path.contains("_timelapse_") {
        progress(ProgressEvent::Log(
            LogLevel::Warn,
            format!(
                "File '{}' appears to be a rotating or timelapse recording, which is not currently supported. \
                 Output may be missing data or incorrect.",
                ubv_path
            ),
        ));
    }

    // Parse the .ubv file
    let mut reader = ubv::reader::open_ubv(Path::new(ubv_path))
        .map_err(|e| format!("Error opening UBV file: {}", e))?;
    let ubv_file = ubv::reader::parse_ubv(&mut reader)
        .map_err(|e| format!("Error parsing UBV file: {}", e))?;

    // Warn if any frame uses chunked packets (not yet supported)
    let has_chunked = ubv_file.partitions.iter().any(|p| {
        p.entries
            .iter()
            .any(|e| matches!(e, PartitionEntry::Frame(f) if f.packet_position != PacketPosition::Single))
    });
    if has_chunked {
        progress(ProgressEvent::Log(
            LogLevel::Warn,
            "This file contains chunked (multi-packet) frames, which have not yet been \
             fully mapped. Output may be corrupt. If you would like to help improve the \
             project, please raise an issue at \
             https://github.com/petergeneric/unifi-protect-remux/issues/new?template=bug_report.md \
             and ideally attach the .ubv file (or the result of ubv-anonymise | gzip)."
                .to_string(),
        ));
    }

    // Resolve video track: auto-detect from first partition if not specified
    let video_track = if config.video_track != 0 {
        config.video_track
    } else {
        match detect_video_track(&ubv_file.partitions) {
            Some(id) => {
                let codec = codec_name_for_track(id).unwrap_or("unknown");
                progress(ProgressEvent::Log(
                    LogLevel::Info,
                    format!("Auto-detected video track: {} ({})", id, codec),
                ));
                id
            }
            None => {
                progress(ProgressEvent::Log(
                    LogLevel::Warn,
                    "No video track found in file".to_string(),
                ));
                0
            }
        }
    };

    // Analyse each partition
    let partitions: Vec<_> = ubv_file
        .partitions
        .iter()
        .map(|p| analysis::analyse(p, config.with_audio, video_track))
        .collect::<io::Result<Vec<_>>>()
        .map_err(|e| format!("Error analysing partitions: {}", e))?;

    if let Some(first) = partitions.first() {
        let track_count = first.video_track_count + first.audio_track_count;
        progress(ProgressEvent::Log(
            LogLevel::Info,
            "First Partition:".to_string(),
        ));
        progress(ProgressEvent::Log(
            LogLevel::Info,
            format!("\tTracks: {}", track_count),
        ));
        progress(ProgressEvent::Log(
            LogLevel::Info,
            format!("\tFrames: {}", first.frames.len()),
        ));

        if let Some(ref vt) = first.video_track {
            let codec = codec_name_for_track(vt.track_id).unwrap_or("unknown");
            progress(ProgressEvent::Log(
                LogLevel::Info,
                format!(
                    "\tVideo: {} ({} fps, {} frames)",
                    codec, vt.nominal_fps, vt.frame_count
                ),
            ));
        }
        if let Some(ref at) = first.audio_track {
            let codec = codec_name_for_track(at.track_id).unwrap_or("unknown");
            progress(ProgressEvent::Log(
                LogLevel::Info,
                format!(
                    "\tAudio: {} ({} Hz, {} frames)",
                    codec, at.clock_rate, at.frame_count
                ),
            ));
        }

        let start_tc = first
            .video_track
            .as_ref()
            .or(first.audio_track.as_ref())
            .and_then(|t| t.start_timecode.as_ref());
        if let Some(tc) = start_tc {
            progress(ProgressEvent::Log(
                LogLevel::Info,
                format!(
                    "\tStart Timecode: {}",
                    tc.to_rfc3339_opts(SecondsFormat::Secs, true)
                ),
            ));
        }
    }

    progress(ProgressEvent::PartitionsFound {
        count: partitions.len(),
    });

    if partitions.is_empty() {
        progress(ProgressEvent::Log(
            LogLevel::Info,
            "No partitions found, nothing to extract".to_string(),
        ));
        progress(ProgressEvent::FileCompleted {
            path: ubv_path.to_string(),
            outputs: result.output_files.clone(),
            errors: result.errors.clone(),
        });
        return Ok(result);
    } else if partitions.len() == 1 {
        progress(ProgressEvent::Log(
            LogLevel::Info,
            "Extracting 1 partition".to_string(),
        ));
    } else {
        progress(ProgressEvent::Log(
            LogLevel::Info,
            format!("Extracting {} partitions", partitions.len()),
        ));
    }

    let force_rate = if config.force_rate > 0 {
        progress(ProgressEvent::Log(
            LogLevel::Info,
            format!(
                "\nFramerate forced by user instruction: using {} fps",
                config.force_rate
            ),
        ));
        Some(config.force_rate)
    } else {
        None
    };

    let total = partitions.len();
    for (partition_idx, partition) in partitions.iter().enumerate() {
        let partition_num = partition_idx + 1;

        progress(ProgressEvent::PartitionStarted {
            index: partition_idx,
            total,
        });

        // Build output filenames
        let output_folder = {
            let f = config.output_folder.trim_end_matches(['/', '\\']);
            if f == "SRC-FOLDER" {
                Path::new(ubv_path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| ".".to_string())
            } else {
                f.to_string()
            }
        };

        // Use config.base_name if provided, otherwise strip extension and
        // trailing Unifi timestamp component from the input filename.
        let base_filename = if let Some(ref name) = config.base_name {
            name.clone()
        } else {
            let stem = Path::new(ubv_path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            match stem.rfind('_') {
                Some(idx) => stem[..idx].to_string(),
                None => stem,
            }
        };

        // Get start timecode for filename
        let tc_str = match get_start_timecode(partition, video_track) {
            Some(tc) => tc
                .to_rfc3339_opts(SecondsFormat::Secs, true)
                .replace(':', "."),
            None => "unknown-time".to_string(),
        };

        let basename = Path::new(&output_folder)
            .join(format!("{}_{}", base_filename, tc_str))
            .to_string_lossy()
            .to_string();

        if config.mp4 {
            let mp4_file = format!("{}.mp4", basename);
            progress(ProgressEvent::Log(
                LogLevel::Info,
                format!("Writing MP4 {}...", mp4_file),
            ));

            if let Err(e) = mp4mux::stream_to_mp4(
                ubv_path,
                partition,
                video_track,
                &mp4_file,
                force_rate,
                config.fast_start,
            ) {
                let _ = std::fs::remove_file(&mp4_file);
                let err_msg = e.to_string();
                progress(ProgressEvent::PartitionError {
                    index: partition_num,
                    error: err_msg.clone(),
                });
                result.errors.push(format!(
                    "Error remuxing partition #{} of {}: {}",
                    partition_num, ubv_path, err_msg
                ));
            } else {
                progress(ProgressEvent::OutputGenerated {
                    path: mp4_file.clone(),
                });
                result.output_files.push(mp4_file);
            }
        } else {
            // Demux to raw bitstream files
            let video_file = if config.with_video && partition.video_track_count > 0 {
                let ext = codec_name_for_track(video_track).unwrap_or("h264");
                Some(format!("{}.{}", basename, ext))
            } else {
                None
            };

            let audio_file = if config.with_audio && partition.audio_track_count > 0 {
                let ext = partition
                    .audio_track
                    .as_ref()
                    .and_then(|at| codec_name_for_track(at.track_id))
                    .unwrap_or("aac");
                Some(format!("{}.{}", basename, ext))
            } else {
                None
            };

            if let Err(e) = demux::demux_partition(
                ubv_path,
                partition,
                video_file.as_deref(),
                video_track,
                audio_file.as_deref(),
            ) {
                if let Some(ref path) = video_file {
                    let _ = std::fs::remove_file(path);
                }
                if let Some(ref path) = audio_file {
                    let _ = std::fs::remove_file(path);
                }

                let err_msg = e.to_string();
                progress(ProgressEvent::PartitionError {
                    index: partition_num,
                    error: err_msg.clone(),
                });
                result.errors.push(format!(
                    "Error remuxing partition #{} of {}: {}",
                    partition_num, ubv_path, err_msg
                ));
            } else {
                if let Some(ref path) = video_file {
                    progress(ProgressEvent::OutputGenerated {
                        path: path.clone(),
                    });
                    result.output_files.push(path.clone());
                }
                if let Some(ref path) = audio_file {
                    progress(ProgressEvent::OutputGenerated {
                        path: path.clone(),
                    });
                    result.output_files.push(path.clone());
                }
            }
        }
    }

    progress(ProgressEvent::FileCompleted {
        path: ubv_path.to_string(),
        outputs: result.output_files.clone(),
        errors: result.errors.clone(),
    });

    Ok(result)
}
