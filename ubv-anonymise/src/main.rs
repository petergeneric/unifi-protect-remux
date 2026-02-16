use clap::Parser;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::PathBuf;
use ubv::reader::open_ubv;
use ubv::record;
use ubv::track;

#[derive(Parser)]
#[command(about = "Strip audio/video/image essence from a .ubv file, preserving record structure")]
struct Args {
    /// Display version and quit
    #[arg(long)]
    version: bool,

    /// Keep SmartEvent metadata (do not anonymise)
    #[arg(long)]
    keep_smart_events: bool,

    /// Input .ubv file
    input: Option<PathBuf>,
    /// Output .ubv file (anonymised copy)
    output: Option<PathBuf>,
}

/// Zero out a region of the file at the given offset and size.
fn zero_region(file: &mut File, offset: u64, size: u32) -> io::Result<()> {
    file.seek(SeekFrom::Start(offset))?;
    let buf = [0u8; 65536];
    let mut remaining = size as usize;
    while remaining > 0 {
        let chunk = remaining.min(buf.len());
        file.write_all(&buf[..chunk])?;
        remaining -= chunk;
    }
    Ok(())
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    if args.version {
        ubv::version::print_cli_version_banner(
            "UBV Anonymise Tool",
            env!("CARGO_PKG_VERSION"),
            env!("RELEASE_VERSION"),
            env!("GIT_COMMIT"),
        );
        return Ok(());
    }

    let input = args.input.ok_or("INPUT is required unless --version is specified")?;
    let output = args.output.ok_or("OUTPUT is required unless --version is specified")?;

    if input.to_string_lossy().ends_with(".ubv.gz") {
        return Err(
            ".ubv.gz input is not supported for anonymisation; provide an uncompressed .ubv file"
                .into(),
        );
    }

    fs::copy(&input, &output)?;

    let mut reader = open_ubv(&input)?;
    let mut out = OpenOptions::new().write(true).open(&output)?;

    let mut records_zeroed: u64 = 0;
    let mut bytes_zeroed: u64 = 0;

    loop {
        let rec = match record::read_record(&mut reader) {
            Ok(Some(r)) => r,
            Ok(None) => break,
            Err(e) => {
                eprintln!("Warning: record parse error: {e}");
                break;
            }
        };

        let should_zero = match track::track_info(rec.track_id) {
            Some(info) if info.is_video() || info.is_audio() => true,
            Some(info)
                if matches!(
                    info.track_type,
                    track::TrackType::Jpeg | track::TrackType::Talkback
                ) =>
            {
                true
            }
            Some(info)
                if matches!(info.track_type, track::TrackType::SmartEvent)
                    && !args.keep_smart_events =>
            {
                true
            }
            _ if rec.track_id == track::TRACK_PARTITION && rec.data_size > 32 => true,
            _ => false,
        };

        if should_zero && rec.data_size > 0 {
            zero_region(&mut out, rec.data_offset, rec.data_size)?;
            records_zeroed += 1;
            bytes_zeroed += rec.data_size as u64;
        }
    }

    println!(
        "Anonymised {} records, zeroed {} bytes ({:.1} MB)",
        records_zeroed,
        bytes_zeroed,
        bytes_zeroed as f64 / (1024.0 * 1024.0)
    );

    Ok(())
}

fn main() {
    let args = Args::parse();
    if let Err(e) = run(args) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
