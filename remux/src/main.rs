use std::path::Path;

use clap::{ArgAction, Parser};

use remux_lib::{LogLevel, ProgressEvent, RemuxConfig};

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

/// Expand glob patterns in the file list. On Unix the shell normally expands
/// globs before the process sees them, but on Windows `cmd.exe` and PowerShell
/// pass the literal pattern (e.g. `*.ubv`) to the program. This function
/// ensures consistent behaviour across platforms.
fn expand_globs(patterns: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    for pattern in patterns {
        // Only attempt glob expansion if the argument contains metacharacters
        // AND does not match an existing file on disk (so that filenames
        // containing '[', '?' etc. are handled correctly).
        let has_glob_chars = pattern.contains('*') || pattern.contains('?') || pattern.contains('[');
        if has_glob_chars && !Path::new(pattern).exists() {
            match glob::glob(pattern) {
                Ok(paths) => {
                    let mut matched = false;
                    for entry in paths.flatten() {
                        result.push(entry.to_string_lossy().to_string());
                        matched = true;
                    }
                    if !matched {
                        // No matches — keep the original so the user gets a
                        // meaningful "file not found" error downstream.
                        result.push(pattern.clone());
                    }
                }
                Err(_) => result.push(pattern.clone()),
            }
        } else {
            result.push(pattern.clone());
        }
    }
    result
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

fn args_to_config(args: &Args) -> RemuxConfig {
    RemuxConfig {
        with_audio: args.with_audio,
        with_video: args.with_video,
        force_rate: args.force_rate,
        fast_start: args.fast_start,
        output_folder: args.output_folder.clone(),
        mp4: args.mp4,
        video_track: args.video_track,
    }
}

/// Forward a ProgressEvent to the log crate.
fn log_progress(event: ProgressEvent) {
    match event {
        ProgressEvent::Log(level, msg) => match level {
            LogLevel::Info => log::info!("{}", msg),
            LogLevel::Warn => log::warn!("{}", msg),
            LogLevel::Error => log::error!("{}", msg),
        },
        ProgressEvent::FileStarted { .. }
        | ProgressEvent::PartitionsFound { .. }
        | ProgressEvent::PartitionStarted { .. } => {}
        ProgressEvent::OutputGenerated { path } => {
            log::info!("Output: {}", path);
        }
        ProgressEvent::PartitionError { index, error } => {
            log::warn!("Error remuxing partition #{}: {}", index, error);
        }
        ProgressEvent::FileCompleted { .. } => {}
    }
}

/// A deferred failure collected when not in fail-fast mode.
#[derive(Debug)]
enum DeferredError {
    /// Error inspecting (opening / parsing / analysing) a .ubv file.
    Inspect { file: String, error: String },
}

impl std::fmt::Display for DeferredError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeferredError::Inspect { file, error } => {
                write!(f, "Failed to inspect {}: {}", file, error)
            }
        }
    }
}

fn remux_cli(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let mut errors: Vec<DeferredError> = Vec::new();
    let files = expand_globs(&args.files);
    let config = args_to_config(args);

    for ubv_path in &files {
        if let Err(e) = process_file(args, &config, ubv_path, &mut errors) {
            if args.fail_fast {
                return Err(e);
            }
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

/// Process a single .ubv file via remux-lib, mapping results back to the
/// CLI's deferred error handling.
fn process_file(
    args: &Args,
    config: &RemuxConfig,
    ubv_path: &str,
    errors: &mut Vec<DeferredError>,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = remux_lib::process_file(ubv_path, config, &mut |event| {
        log_progress(event);
    });

    match result {
        Ok(file_result) => {
            for err_msg in &file_result.errors {
                if args.fail_fast {
                    log::error!("{}", err_msg);
                } else {
                    // Parse the error message to extract partition number
                    errors.push(DeferredError::Inspect {
                        file: ubv_path.to_string(),
                        error: err_msg.clone(),
                    });
                }
            }
            if !file_result.errors.is_empty() && args.fail_fast {
                return Err(file_result.errors[0].clone().into());
            }
            Ok(())
        }
        Err(e) => {
            let msg = e.to_string();
            if args.fail_fast {
                log::error!("{}: {}", ubv_path, msg);
            } else {
                log::warn!("{}: {}", ubv_path, msg);
                errors.push(DeferredError::Inspect {
                    file: ubv_path.to_string(),
                    error: msg.clone(),
                });
            }
            Err(msg.into())
        }
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
        let config = args_to_config(&args);
        let mut errors = Vec::new();

        let result = process_file(&args, &config, "nonexistent.ubv", &mut errors);
        assert!(result.is_err());
        assert_eq!(errors.len(), 1);
        let DeferredError::Inspect { file, error } = &errors[0];
        assert_eq!(file, "nonexistent.ubv");
        assert!(error.contains("Error opening UBV file"), "got: {}", error);
    }

    #[test]
    fn fail_fast_mode_does_not_push_to_errors() {
        let mut args = base_args();
        args.fail_fast = true;
        let config = args_to_config(&args);
        let mut errors = Vec::new();

        let result = process_file(&args, &config, "nonexistent.ubv", &mut errors);
        assert!(result.is_err());
        assert!(errors.is_empty(), "fail-fast should not push deferred errors");
    }

    #[test]
    fn deferred_mode_continues_past_failures() {
        let args = base_args();
        let config = args_to_config(&args);
        let mut errors = Vec::new();

        let paths = vec!["nonexistent_a.ubv", "nonexistent_b.ubv"];
        for path in &paths {
            let _ = process_file(&args, &config, path, &mut errors);
        }

        // Both files should have produced an error
        assert_eq!(errors.len(), 2);
        let DeferredError::Inspect { file, .. } = &errors[0];
        assert_eq!(file, "nonexistent_a.ubv");
        let DeferredError::Inspect { file, .. } = &errors[1];
        assert_eq!(file, "nonexistent_b.ubv");
    }
}
