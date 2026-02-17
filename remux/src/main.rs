mod analysis;
mod demux;
mod mp4mux;
mod probe;

use std::io;
use std::path::Path;

use chrono::SecondsFormat;
use clap::{ArgAction, Parser};

use ubv::format::PacketPosition;
use ubv::partition::{Partition, PartitionEntry};
use ubv::track::{is_video_track, track_info};

/// UBV Remux Tool — converts .ubv files to MP4.
#[derive(Parser)]
#[command(name = "remux")]
struct Args {
    /// Extract audio stream?
    #[arg(long = "with-audio", default_value_t = true, action = ArgAction::Set)]
    with_audio: bool,

    /// Extract video stream?
    #[arg(long = "with-video", default_value_t = true, action = ArgAction::Set)]
    with_video: bool,

    /// Force a particular video framerate (0 = auto Variable Framerate, otherwise force CFR)
    #[arg(long = "force-rate", default_value_t = 0)]
    force_rate: u32,

    /// If true, generated MP4 files will have faststart enabled for better streaming. Increases remux IO cost
    #[arg(long = "fast-start", default_value_t = false, action = ArgAction::Set)]
    fast_start: bool,

    /// Output directory ("SRC-FOLDER" = alongside .ubv files)
    #[arg(long = "output-folder", default_value = "./")]
    output_folder: String,

    /// Create MP4 output
    #[arg(long = "mp4", default_value_t = true, action = ArgAction::Set)]
    mp4: bool,

    /// Video track number (7 = H.264, 1003 = HEVC, 1004 = AV1, 0 = auto-detect)
    #[arg(long = "video-track", default_value_t = 0)]
    video_track: u16,

    /// Stop on the first error instead of continuing and reporting failures at the end
    #[arg(long = "fail-fast", default_value_t = false, action = ArgAction::Set)]
    fail_fast: bool,

    /// Display version and quit
    #[arg(long = "version")]
    version: bool,

    /// Input .ubv files
    files: Vec<String>,
}

/// Convert known single-dash flags to double-dash for clap compatibility.
/// Handles both `-flag value` and `-flag=value` forms.
fn normalise_args(args: Vec<String>) -> Vec<String> {
    let known_flags = [
        "-with-audio",
        "-with-video",
        "-force-rate",
        "-fast-start",
        "-output-folder",
        "-mp4",
        "-video-track",
        "-fail-fast",
        "-version",
    ];

    args.into_iter()
        .map(|arg| {
            for flag in &known_flags {
                if arg == *flag {
                    return format!("-{}", flag);
                }
                let prefix = format!("{}=", flag);
                if arg.starts_with(&prefix) {
                    return format!("-{}", arg);
                }
            }
            arg
        })
        .collect()
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    let raw_args: Vec<String> = std::env::args().collect();
    let normalised = normalise_args(raw_args);
    let args = Args::parse_from(normalised);

    if let Err(e) = run(&args) {
        log::error!("{}", e);
        std::process::exit(1);
    }
}

fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    if args.version {
        ubv::version::print_cli_version_banner(
            "UBV Remux Tool",
            env!("CARGO_PKG_VERSION"),
            env!("RELEASE_VERSION"),
            env!("GIT_COMMIT"),
        );
        return Ok(());
    }

    validate_args(args)?;
    remux_cli(args)
}

fn validate_args(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    if args.files.is_empty() {
        return Err("Expected at least one .ubv file as input!".into());
    }

    if !args.with_audio && !args.with_video {
        return Err("Must enable extraction of at least one of: audio, video!".into());
    }

    if args.mp4 && !args.with_video {
        return Err(
            "MP4 output requires video; --with-video=false is not supported with --mp4=true"
                .into(),
        );
    }

    Ok(())
}

/// A deferred failure collected when not in fail-fast mode.
#[derive(Debug)]
enum DeferredError {
    /// Error inspecting (opening / parsing / analysing) a .ubv file.
    Inspect {
        file: String,
        error: String,
    },
    /// Error remuxing a specific partition within a .ubv file.
    Partition {
        file: String,
        partition: usize,
        error: String,
    },
}

impl std::fmt::Display for DeferredError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeferredError::Inspect { file, error } => {
                write!(f, "Failed to inspect {}: {}", file, error)
            }
            DeferredError::Partition {
                file,
                partition,
                error,
            } => {
                write!(
                    f,
                    "Failed to remux partition #{} of {}: {}",
                    partition, file, error
                )
            }
        }
    }
}

fn remux_cli(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let mut errors: Vec<DeferredError> = Vec::new();

    for ubv_path in &args.files {
        if let Err(e) = process_file(args, ubv_path, &mut errors) {
            if args.fail_fast {
                return Err(e);
            }
            // In deferred mode the error was already pushed onto `errors`
            // inside `process_file`, so we just continue.
        }
    }

    if !errors.is_empty() {
        log::error!("");
        log::error!("OPERATION COMPLETED WITH ERRORS:");
        for e in &errors {
            log::error!("  {}", e);
        }
        return Err(format!("{} error(s) encountered during processing", errors.len()).into());
    }

    Ok(())
}

/// Record an inspection-phase error. In deferred mode, logs a warning and
/// pushes the error onto the list for the final summary. In fail-fast mode
/// the error propagates immediately so we log at error level and skip the push.
fn record_inspect_error(
    ubv_path: &str,
    msg: String,
    fail_fast: bool,
    errors: &mut Vec<DeferredError>,
) -> Box<dyn std::error::Error> {
    if fail_fast {
        log::error!("{}: {}", ubv_path, msg);
    } else {
        log::warn!("{}: {}", ubv_path, msg);
        errors.push(DeferredError::Inspect {
            file: ubv_path.to_string(),
            error: msg.clone(),
        });
    }
    msg.into()
}

/// Process a single .ubv file: inspect, analyse, and remux each partition.
///
/// In fail-fast mode errors propagate immediately. Otherwise they are appended
/// to `errors` and processing continues.
fn process_file(
    args: &Args,
    ubv_path: &str,
    errors: &mut Vec<DeferredError>,
) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Analysing {}", ubv_path);

    if ubv_path.contains("_2_rotating_") || ubv_path.contains("_timelapse_") {
        log::warn!(
            "File '{}' appears to be a rotating or timelapse recording, which is not currently supported. \
             Output may be missing data or incorrect.",
            ubv_path
        );
    }

    // Parse the .ubv file
    let mut reader = ubv::reader::open_ubv(Path::new(ubv_path)).map_err(|e| {
        record_inspect_error(ubv_path, format!("Error opening UBV file: {}", e), args.fail_fast, errors)
    })?;
    let ubv_file = ubv::reader::parse_ubv(&mut reader).map_err(|e| {
        record_inspect_error(ubv_path, format!("Error parsing UBV file: {}", e), args.fail_fast, errors)
    })?;

    // Warn if any frame uses chunked packets (not yet supported)
    let has_chunked = ubv_file.partitions.iter().any(|p| {
        p.entries.iter().any(|e| matches!(e, PartitionEntry::Frame(f) if f.packet_position != PacketPosition::Single))
    });
    if has_chunked {
        log::warn!(
            "This file contains chunked (multi-packet) frames, which have not yet been \
             fully mapped. Output may be corrupt. If you would like to help improve the \
             project, please raise an issue at \
             https://github.com/petergeneric/unifi-protect-remux/issues/new?template=bug_report.md \
             and ideally attach the .ubv file (or the result of ubv-anonymise | gzip)."
        );
    }

    // Resolve video track: auto-detect from first partition if not specified
    let video_track = if args.video_track != 0 {
        args.video_track
    } else {
        match detect_video_track(&ubv_file.partitions) {
            Some(id) => {
                let codec = codec_name_for_track(id).unwrap_or("unknown");
                log::info!("Auto-detected video track: {} ({})", id, codec);
                id
            }
            None => {
                log::warn!("No video track found in file");
                0
            }
        }
    };

    // Analyse each partition
    let partitions: Vec<_> = ubv_file
        .partitions
        .iter()
        .map(|p| analysis::analyse(p, args.with_audio, video_track))
        .collect::<io::Result<Vec<_>>>()
        .map_err(|e| {
            record_inspect_error(
                ubv_path,
                format!("Error analysing partitions: {}", e),
                args.fail_fast,
                errors,
            )
        })?;

    if let Some(first) = partitions.first() {
        log::info!("First Partition:");
        let track_count =
            first.video_track_count + first.audio_track_count;
        log::info!("\tTracks: {}", track_count);
        log::info!("\tFrames: {}", first.frames.len());

        if let Some(ref vt) = first.video_track {
            let codec = codec_name_for_track(vt.track_id).unwrap_or("unknown");
            log::info!(
                "\tVideo: {} ({} fps, {} frames)",
                codec, vt.nominal_fps, vt.frame_count
            );
        }
        if let Some(ref at) = first.audio_track {
            let codec = codec_name_for_track(at.track_id).unwrap_or("unknown");
            log::info!(
                "\tAudio: {} ({} Hz, {} frames)",
                codec, at.clock_rate, at.frame_count
            );
        }

        let start_tc = first
            .video_track
            .as_ref()
            .or(first.audio_track.as_ref())
            .and_then(|t| t.start_timecode.as_ref());
        if let Some(tc) = start_tc {
            log::info!(
                "\tStart Timecode: {}",
                tc.to_rfc3339_opts(SecondsFormat::Secs, true)
            );
        }
    }

    if partitions.is_empty() {
        log::info!("No partitions found, nothing to extract");
        return Ok(());
    } else if partitions.len() == 1 {
        log::info!("Extracting 1 partition");
    } else {
        log::info!("Extracting {} partitions", partitions.len());
    }

    let force_rate = if args.force_rate > 0 {
        log::info!(
            "\nFramerate forced by user instruction: using {} fps",
            args.force_rate
        );
        Some(args.force_rate)
    } else {
        None
    };

    for (partition_idx, partition) in partitions.iter().enumerate() {
        let partition_num = partition_idx + 1;

        // Build output filenames
        let output_folder = {
            let f = args.output_folder.trim_end_matches(['/', '\\']);
            if f == "SRC-FOLDER" {
                Path::new(ubv_path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| ".".to_string())
            } else {
                f.to_string()
            }
        };

        // Strip extension and trailing Unifi timestamp component
        let base_filename = Path::new(ubv_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let base_filename = match base_filename.rfind('_') {
            Some(idx) => base_filename[..idx].to_string(),
            None => base_filename,
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

        if args.mp4 {
            // Stream directly from UBV to MP4 — no intermediate files
            let mp4_file = format!("{}.mp4", basename);
            log::info!("Writing MP4 {}...", mp4_file);

            if let Err(e) = mp4mux::stream_to_mp4(
                ubv_path,
                partition,
                video_track,
                &mp4_file,
                force_rate,
                args.fast_start,
            ) {
                // Clean up partial output file
                let _ = std::fs::remove_file(&mp4_file);

                if args.fail_fast {
                    return Err(e.into());
                }
                log::warn!(
                    "Error remuxing partition #{} of {}: {}",
                    partition_num, ubv_path, e
                );
                errors.push(DeferredError::Partition {
                    file: ubv_path.to_string(),
                    partition: partition_num,
                    error: e.to_string(),
                });
            }
        } else {
            // Demux to raw bitstream files
            let video_file = if args.with_video && partition.video_track_count > 0 {
                let ext = codec_name_for_track(video_track).unwrap_or("h264");
                Some(format!("{}.{}", basename, ext))
            } else {
                None
            };

            let audio_file = if args.with_audio && partition.audio_track_count > 0 {
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
                // Clean up partial output files
                if let Some(ref path) = video_file {
                    let _ = std::fs::remove_file(path);
                }
                if let Some(ref path) = audio_file {
                    let _ = std::fs::remove_file(path);
                }

                if args.fail_fast {
                    return Err(e.into());
                }
                log::warn!(
                    "Error remuxing partition #{} of {}: {}",
                    partition_num, ubv_path, e
                );
                errors.push(DeferredError::Partition {
                    file: ubv_path.to_string(),
                    partition: partition_num,
                    error: e.to_string(),
                });
            }
        }
    }

    Ok(())
}

/// Find the first video track ID across all partitions.
fn detect_video_track(partitions: &[Partition]) -> Option<u16> {
    partitions.iter().find_map(|p| {
        p.entries.iter().find_map(|entry| match entry {
            PartitionEntry::Frame(f) if is_video_track(f.header.track_id) => Some(f.header.track_id),
            _ => None,
        })
    })
}

/// Derive codec name from a track ID via track_info.
fn codec_name_for_track(track_id: u16) -> Option<&'static str> {
    track_info(track_id)
        .and_then(|ti| ti.codec)
        .map(|ci| ci.codec_name)
}

fn get_start_timecode(
    partition: &analysis::AnalysedPartition,
    video_track_num: u16,
) -> Option<chrono::DateTime<chrono::Utc>> {
    if partition.video_track_count == 0 {
        // No video — use audio track timecode
        partition.audio_track.as_ref().and_then(|at| at.start_timecode)
    } else {
        partition
            .video_track
            .as_ref()
            .filter(|vt| vt.track_id == video_track_num)
            .and_then(|vt| vt.start_timecode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_args() -> Args {
        Args {
            with_audio: true,
            with_video: true,
            force_rate: 0,
            fast_start: false,
            output_folder: "./".to_string(),
            mp4: true,
            video_track: 0,
            fail_fast: false,
            version: false,
            files: vec!["dummy.ubv".to_string()],
        }
    }

    #[test]
    fn validate_args_rejects_mp4_audio_only() {
        let mut args = base_args();
        args.with_video = false;
        args.with_audio = true;
        args.mp4 = true;

        let err = validate_args(&args).unwrap_err().to_string();
        assert!(err.contains("MP4 output requires video"));
    }

    #[test]
    fn validate_args_allows_audio_only_when_not_mp4() {
        let mut args = base_args();
        args.with_video = false;
        args.with_audio = true;
        args.mp4 = false;

        assert!(validate_args(&args).is_ok());
    }

    #[test]
    fn deferred_mode_collects_inspect_error() {
        let args = base_args();
        let mut errors = Vec::new();

        let result = process_file(&args, "nonexistent.ubv", &mut errors);
        assert!(result.is_err());
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            DeferredError::Inspect { file, error } => {
                assert_eq!(file, "nonexistent.ubv");
                assert!(error.contains("Error opening UBV file"), "got: {}", error);
            }
            other => panic!("expected Inspect error, got: {:?}", other),
        }
    }

    #[test]
    fn fail_fast_mode_does_not_push_to_errors() {
        let mut args = base_args();
        args.fail_fast = true;
        let mut errors = Vec::new();

        let result = process_file(&args, "nonexistent.ubv", &mut errors);
        assert!(result.is_err());
        assert!(errors.is_empty(), "fail-fast should not push deferred errors");
    }

    #[test]
    fn deferred_mode_continues_past_failures() {
        let mut args = base_args();
        args.files = vec![
            "nonexistent_a.ubv".to_string(),
            "nonexistent_b.ubv".to_string(),
        ];

        // Run both files through remux_cli in deferred mode
        let mut errors = Vec::new();
        for path in &args.files {
            let _ = process_file(&args, path, &mut errors);
        }

        // Both files should have produced an error
        assert_eq!(errors.len(), 2);
        match &errors[0] {
            DeferredError::Inspect { file, .. } => assert_eq!(file, "nonexistent_a.ubv"),
            other => panic!("expected Inspect for file a, got: {:?}", other),
        }
        match &errors[1] {
            DeferredError::Inspect { file, .. } => assert_eq!(file, "nonexistent_b.ubv"),
            other => panic!("expected Inspect for file b, got: {:?}", other),
        }
    }
}
