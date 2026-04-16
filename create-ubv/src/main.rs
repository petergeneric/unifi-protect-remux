use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use create_ubv::{synth_from_mp4, SynthConfig};

#[derive(Parser)]
#[command(
    about = "Synthesise a .ubv file from a source MP4 (for test fixtures)",
    version = concat!(env!("GIT_VERSION"), " (", env!("GIT_COMMIT"), ")")
)]
struct Args {
    /// Source MP4 file
    input: PathBuf,
    /// Destination .ubv file
    output: PathBuf,
    /// Wall-clock start time, UTC seconds since the Unix epoch (default 2024-01-01T00:00:00Z)
    #[arg(long)]
    wall_clock_secs: Option<u32>,
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    let mut config = SynthConfig::default();
    if let Some(ts) = args.wall_clock_secs {
        config.wall_clock_secs = ts;
    }

    match synth_from_mp4(&args.input, &args.output, &config) {
        Ok(()) => {
            log::info!("Wrote {}", args.output.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("create-ubv: {e}");
            ExitCode::FAILURE
        }
    }
}
