mod analysis;
mod demux;
mod mp4mux;

use std::io;
use std::path::Path;

use chrono::SecondsFormat;
use clap::{ArgAction, Parser};

use ubv::partition::{Partition, PartitionEntry};
use ubv::track::{is_video_track, track_info};

/// UBV Remux Tool — converts .ubv files to MP4.
#[derive(Parser)]
#[command(name = "remux")]
struct Args {
    /// Extract audio streams
    #[arg(long = "with-audio", default_value_t = true, action = ArgAction::Set)]
    with_audio: bool,

    /// Extract video streams
    #[arg(long = "with-video", default_value_t = true, action = ArgAction::Set)]
    with_video: bool,

    /// Override detected framerate (0 = auto-detect)
    #[arg(long = "force-rate", default_value_t = 0)]
    force_rate: u32,

    /// Output directory ("SRC-FOLDER" = alongside .ubv files)
    #[arg(long = "output-folder", default_value = "./")]
    output_folder: String,

    /// Create MP4 output
    #[arg(long = "mp4", default_value_t = true, action = ArgAction::Set)]
    mp4: bool,

    /// Video track number (7 = H.264, 1003 = HEVC, 1004 = AV1, 0 = auto-detect)
    #[arg(long = "video-track", default_value_t = 0)]
    video_track: u16,

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
        "-output-folder",
        "-mp4",
        "-video-track",
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
        println!("UBV Remux Tool");
        println!("Copyright (c) Peter Wright 2020-2026");
        println!("https://github.com/petergeneric/unifi-protect-remux");
        println!();

        let release = env!("RELEASE_VERSION");
        let commit = env!("GIT_COMMIT");
        if !release.is_empty() {
            println!("\tVersion:     {}", release);
        } else {
            println!("\tGit commit:  {}", commit);
        }
        return Ok(());
    }

    if args.files.is_empty() {
        return Err("Expected at least one .ubv file as input!".into());
    }

    if !args.with_audio && !args.with_video {
        return Err("Must enable extraction of at least one of: audio, video!".into());
    }

    remux_cli(args)
}

fn remux_cli(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    for ubv_path in &args.files {
        log::info!("Analysing {}", ubv_path);

        // Parse the .ubv file
        let mut reader = ubv::reader::open_ubv(Path::new(ubv_path))
            .map_err(|e| io::Error::new(e.kind(), format!("Error opening UBV file {}: {}", ubv_path, e)))?;
        let ubv_file = ubv::reader::parse_ubv(&mut reader)
            .map_err(|e| format!("Error parsing UBV file {}: {}", ubv_path, e))?;

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
            .collect::<io::Result<Vec<_>>>()?;

        log::info!("\n\nAnalysis complete!");
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
                    codec, vt.rate, vt.frame_count
                );
            }
            if let Some(ref at) = first.audio_track {
                let codec = codec_name_for_track(at.track_id).unwrap_or("unknown");
                log::info!(
                    "\tAudio: {} ({} Hz, {} frames)",
                    codec, at.rate, at.frame_count
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

        log::info!("\n\nExtracting {} partitions", partitions.len());

        let force_rate = if args.force_rate > 0 {
            log::info!(
                "\nFramerate forced by user instruction: using {} fps",
                args.force_rate
            );
            Some(args.force_rate)
        } else {
            None
        };

        for partition in &partitions {
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

            // Determine output file paths
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

            // Demux
            demux::demux_partition(
                ubv_path,
                partition,
                video_file.as_deref(),
                video_track,
                audio_file.as_deref(),
            )?;

            // Mux to MP4
            if args.mp4 {
                let mp4_file = format!("{}.mp4", basename);
                log::info!("\nWriting MP4 {}...", mp4_file);

                mp4mux::mux(
                    partition,
                    video_file.as_deref(),
                    video_track,
                    audio_file.as_deref(),
                    &mp4_file,
                    force_rate,
                )?;

                // Delete intermediate files
                if let Some(ref vf) = video_file {
                    if let Err(e) = std::fs::remove_file(vf) {
                        log::warn!("Could not delete {}: {}", vf, e);
                    }
                }
                if let Some(ref af) = audio_file {
                    if let Err(e) = std::fs::remove_file(af) {
                        log::warn!("Could not delete {}: {}", af, e);
                    }
                }
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
